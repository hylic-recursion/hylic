//! WorkerDeque<T>: typed Chase-Lev work-stealing deque.
//!
//! Owner: push to bottom (LIFO), pop from bottom (LIFO). Single-thread, no atomics.
//! Stealers: steal from top (FIFO). CAS on top for contention resolution.
//!
//! Fixed capacity (power of 2). Tasks stored inline — no Box, no indirection.
//!
//! Cache-padded: bottom and top are on separate 128-byte cache lines.
//! Push/pop only touch the owner's line; steal only touches the stealer's
//! line. No false sharing under concurrent push+steal.
//!
//! Buffer uses ManuallyDrop<T> to prevent double-free on speculative reads:
//! steal() and pop() read before the ownership CAS. On CAS failure, the
//! ManuallyDrop wrapper ensures the speculative copy is NOT dropped — the
//! winning thread takes ownership and drops it.

use std::mem::{ManuallyDrop, MaybeUninit};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicIsize, Ordering, fence};

/// A slot in the deque buffer. ManuallyDrop prevents the speculative-read
/// double-free: reads produce ManuallyDrop<T> which has a no-op destructor.
/// Only the thread that wins the ownership race calls into_inner().
struct Slot<T>(UnsafeCell<MaybeUninit<ManuallyDrop<T>>>);

impl<T> Slot<T> {
    fn new() -> Self { Slot(UnsafeCell::new(MaybeUninit::uninit())) }

    /// # Safety
    /// Caller must ensure no other thread reads this slot during the write.
    /// In the deque, push() writes before incrementing bottom, and no
    /// stealer reads beyond bottom.
    unsafe fn write(&self, value: T) {
        // SAFETY: forwarded from fn contract.
        unsafe { (*self.0.get()).write(ManuallyDrop::new(value)); }
    }

    /// # Safety
    /// Slot must have been initialized by a prior write() and not
    /// already consumed. The ManuallyDrop wrapper tolerates aliased
    /// speculative reads: only the CAS winner eventually calls
    /// `into_inner` to take ownership.
    unsafe fn read_speculative(&self) -> ManuallyDrop<T> {
        // SAFETY: forwarded from fn contract.
        unsafe { (*self.0.get()).assume_init_read() }
    }
}

#[repr(align(128))]
struct CachePad<T>(T);

pub(crate) struct WorkerDeque<T> {
    buffer: Box<[Slot<T>]>,
    mask: isize,
    bottom: CachePad<AtomicIsize>,
    top: CachePad<AtomicIsize>,
}

unsafe impl<T: Send> Send for WorkerDeque<T> {}
unsafe impl<T: Send> Sync for WorkerDeque<T> {}

impl<T> WorkerDeque<T> {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        let buffer: Vec<Slot<T>> = (0..cap).map(|_| Slot::new()).collect();
        WorkerDeque {
            buffer: buffer.into_boxed_slice(),
            mask: (cap - 1) as isize,
            bottom: CachePad(AtomicIsize::new(0)),
            top: CachePad(AtomicIsize::new(0)),
        }
    }

    /// Owner pushes to the bottom. Returns Err(item) if full.
    pub fn push(&self, item: T) -> Result<(), T> {
        let b = self.bottom.0.load(Ordering::Relaxed);
        let t = self.top.0.load(Ordering::Acquire);
        if b - t > self.mask {
            return Err(item);
        }
        // SAFETY: we are the sole owner (push is single-threaded); the
        // slot at position `b` is not yet visible to stealers because
        // `bottom` has not been incremented.
        unsafe { self.buffer[(b & self.mask) as usize].write(item); }
        fence(Ordering::Release);
        self.bottom.0.store(b + 1, Ordering::Relaxed);
        Ok(())
    }

    /// Owner pops from the bottom (LIFO). No CAS in the common case.
    pub fn pop(&self) -> Option<T> {
        let b = self.bottom.0.load(Ordering::Relaxed) - 1;
        self.bottom.0.store(b, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        let t = self.top.0.load(Ordering::Relaxed);

        if t <= b {
            // SAFETY: the slot at position `b` was written by a prior
            // push and never consumed (pop is single-threaded; steal
            // operates on `top`, not `bottom`). If t < b the slot
            // belongs to us; if t == b a stealer may race but the
            // ManuallyDrop wrapper makes the speculative read safe.
            let item = unsafe { self.buffer[(b & self.mask) as usize].read_speculative() };
            if t == b {
                if self.top.0.compare_exchange(t, t + 1, Ordering::SeqCst, Ordering::Relaxed).is_err() {
                    self.bottom.0.store(t + 1, Ordering::Relaxed);
                    return None;
                }
                self.bottom.0.store(t + 1, Ordering::Relaxed);
            }
            Some(ManuallyDrop::into_inner(item))
        } else {
            self.bottom.0.store(t, Ordering::Relaxed);
            None
        }
    }

    /// Stealer takes from the top (FIFO). CAS for contention.
    pub fn steal(&self) -> Option<T> {
        loop {
            let t = self.top.0.load(Ordering::Acquire);
            fence(Ordering::SeqCst);
            let b = self.bottom.0.load(Ordering::Acquire);

            if t >= b {
                return None;
            }

            // SAFETY: slot at position `t` was written by a push before
            // bottom reached `t + 1`. The read is speculative — the
            // ManuallyDrop wrapper makes a losing CAS harmless.
            let item = unsafe { self.buffer[(t & self.mask) as usize].read_speculative() };
            if self.top.0.compare_exchange_weak(t, t + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                return Some(ManuallyDrop::into_inner(item));
            }
        }
    }

}

impl<T> Drop for WorkerDeque<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn push_pop_lifo() {
        let d = WorkerDeque::new(8);
        assert!(d.push(1).is_ok());
        assert!(d.push(2).is_ok());
        assert!(d.push(3).is_ok());
        assert_eq!(d.pop(), Some(3));
        assert_eq!(d.pop(), Some(2));
        assert_eq!(d.pop(), Some(1));
        assert_eq!(d.pop(), None);
    }

    #[test]
    fn steal_fifo() {
        let d = WorkerDeque::new(8);
        d.push(1).unwrap(); d.push(2).unwrap(); d.push(3).unwrap();
        assert_eq!(d.steal(), Some(1));
        assert_eq!(d.steal(), Some(2));
        assert_eq!(d.steal(), Some(3));
        assert_eq!(d.steal(), None);
    }

    #[test]
    fn push_full() {
        let d = WorkerDeque::new(4);
        assert!(d.push(1).is_ok());
        assert!(d.push(2).is_ok());
        assert!(d.push(3).is_ok());
        assert!(d.push(4).is_ok());
        assert_eq!(d.push(5), Err(5));
    }

    #[test]
    fn owner_pop_vs_stealer() {
        for _ in 0..500 {
            let d = Arc::new(WorkerDeque::new(8));
            d.push(42).unwrap();
            let d2 = d.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();
            let stealer = std::thread::spawn(move || {
                b2.wait();
                d2.steal()
            });
            barrier.wait();
            let popped = d.pop();
            let stolen = stealer.join().unwrap();
            match (popped, stolen) {
                (Some(42), None) | (None, Some(42)) => {}
                other => panic!("invalid: {:?}", other),
            }
        }
    }

    #[test]
    fn no_double_free_on_race() {
        use std::sync::atomic::AtomicU64;
        for _ in 0..500 {
            let d = Arc::new(WorkerDeque::new(8));
            let counter = Arc::new(AtomicU64::new(0));
            let c2 = counter.clone();
            d.push(c2).unwrap();

            let d2 = d.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();
            let stealer = std::thread::spawn(move || {
                b2.wait();
                d2.steal()
            });
            barrier.wait();
            let popped = d.pop();
            let stolen = stealer.join().unwrap();

            match (&popped, &stolen) {
                (Some(_), None) | (None, Some(_)) => {}
                other => panic!("both or neither: {:?}", other),
            }
            drop(popped);
            drop(stolen);
        }
    }

    #[test]
    fn concurrent_push_steal() {
        let d = Arc::new(WorkerDeque::new(256));
        let n = 5000usize;
        let stolen = Arc::new(AtomicUsize::new(0));

        let d2 = d.clone();
        let s2 = stolen.clone();
        let stealer = std::thread::spawn(move || {
            loop {
                if d2.steal().is_some() {
                    if s2.fetch_add(1, Ordering::Relaxed) + 1 >= n {
                        return;
                    }
                } else if s2.load(Ordering::Relaxed) >= n {
                    return;
                }
                std::hint::spin_loop();
            }
        });

        let mut pushed = 0;
        while stolen.load(Ordering::Relaxed) < n {
            if pushed < n * 2 {
                if d.push(pushed).is_ok() { pushed += 1; }
            }
            if d.pop().is_some() {
                stolen.fetch_add(1, Ordering::Relaxed);
            }
        }
        stealer.join().unwrap();
    }
}
