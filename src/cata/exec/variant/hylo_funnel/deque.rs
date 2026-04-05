//! WorkerDeque<T>: typed Chase-Lev work-stealing deque.
//!
//! Owner: push to bottom (LIFO), pop from bottom (LIFO). Single-thread, no atomics.
//! Stealers: steal from top (FIFO). CAS on top for contention resolution.
//!
//! Fixed capacity (power of 2). Tasks stored inline — no Box, no indirection.
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

    /// Write a value into the slot.
    unsafe fn write(&self, value: T) {
        unsafe { (*self.0.get()).write(ManuallyDrop::new(value)); }
    }

    /// Speculative read: copies the ManuallyDrop<T> out. Does NOT drop T.
    /// Caller must call into_inner() on the result to take ownership,
    /// or simply let the ManuallyDrop drop (no-op) if they lost the race.
    unsafe fn read_speculative(&self) -> ManuallyDrop<T> {
        unsafe { (*self.0.get()).assume_init_read() }
    }
}

pub struct WorkerDeque<T> {
    buffer: Box<[Slot<T>]>,
    mask: isize,
    bottom: AtomicIsize,
    top: AtomicIsize,
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
            bottom: AtomicIsize::new(0),
            top: AtomicIsize::new(0),
        }
    }

    /// Owner pushes to the bottom. No CAS — single writer.
    /// Returns false if full.
    pub fn push(&self, item: T) -> bool {
        let b = self.bottom.load(Ordering::Relaxed);
        let t = self.top.load(Ordering::Acquire);
        if b - t > self.mask {
            return false;
        }
        unsafe { self.buffer[(b & self.mask) as usize].write(item); }
        fence(Ordering::Release);
        self.bottom.store(b + 1, Ordering::Relaxed);
        true
    }

    /// Owner pops from the bottom (LIFO). No CAS in the common case.
    pub fn pop(&self) -> Option<T> {
        let b = self.bottom.load(Ordering::Relaxed) - 1;
        self.bottom.store(b, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        let t = self.top.load(Ordering::Relaxed);

        if t <= b {
            let item = unsafe { self.buffer[(b & self.mask) as usize].read_speculative() };
            if t == b {
                // Last item — race with stealers.
                if self.top.compare_exchange(t, t + 1, Ordering::SeqCst, Ordering::Relaxed).is_err() {
                    // Stealer won. item is ManuallyDrop — no-op drop, no double free.
                    self.bottom.store(t + 1, Ordering::Relaxed);
                    return None;
                }
                self.bottom.store(t + 1, Ordering::Relaxed);
            }
            // We own the value. Take it out of ManuallyDrop.
            Some(ManuallyDrop::into_inner(item))
        } else {
            self.bottom.store(t, Ordering::Relaxed);
            None
        }
    }

    /// Stealer takes from the top (FIFO). CAS for contention.
    pub fn steal(&self) -> Option<T> {
        loop {
            let t = self.top.load(Ordering::Acquire);
            fence(Ordering::SeqCst);
            let b = self.bottom.load(Ordering::Acquire);

            if t >= b {
                return None;
            }

            let item = unsafe { self.buffer[(t & self.mask) as usize].read_speculative() };
            if self.top.compare_exchange_weak(t, t + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                // We own the value. Take it out of ManuallyDrop.
                return Some(ManuallyDrop::into_inner(item));
            }
            // CAS failed. item is ManuallyDrop — no-op drop, no double free.
        }
    }


    pub fn len(&self) -> usize {
        let t = self.top.load(Ordering::Relaxed);
        let b = self.bottom.load(Ordering::Relaxed);
        (b - t).max(0) as usize
    }
}

impl<T> Drop for WorkerDeque<T> {
    fn drop(&mut self) {
        // Drain remaining items, properly dropping each.
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
        assert!(d.push(1));
        assert!(d.push(2));
        assert!(d.push(3));
        assert_eq!(d.pop(), Some(3));
        assert_eq!(d.pop(), Some(2));
        assert_eq!(d.pop(), Some(1));
        assert_eq!(d.pop(), None);
    }

    #[test]
    fn steal_fifo() {
        let d = WorkerDeque::new(8);
        d.push(1); d.push(2); d.push(3);
        assert_eq!(d.steal(), Some(1));
        assert_eq!(d.steal(), Some(2));
        assert_eq!(d.steal(), Some(3));
        assert_eq!(d.steal(), None);
    }

    #[test]
    fn push_full() {
        let d = WorkerDeque::new(4);
        assert!(d.push(1));
        assert!(d.push(2));
        assert!(d.push(3));
        assert!(d.push(4));
        assert!(!d.push(5));
    }

    #[test]
    fn owner_pop_vs_stealer() {
        for _ in 0..500 {
            let d = Arc::new(WorkerDeque::new(8));
            d.push(42);
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

    /// Verify no double-free with types that have Drop.
    #[test]
    fn no_double_free_on_race() {
        use std::sync::atomic::AtomicU64;
        for _ in 0..500 {
            let d = Arc::new(WorkerDeque::new(8));
            let counter = Arc::new(AtomicU64::new(0));
            let c2 = counter.clone();
            d.push(c2); // Arc clone: refcount = 2 (counter + c2 in deque)

            let d2 = d.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();
            let stealer = std::thread::spawn(move || {
                b2.wait();
                d2.steal() // might get the Arc
            });
            barrier.wait();
            let popped = d.pop(); // might get the Arc
            let stolen = stealer.join().unwrap();

            // Exactly one got it. The other got None.
            // If double-free occurred, refcount would underflow → crash.
            match (&popped, &stolen) {
                (Some(_), None) | (None, Some(_)) => {}
                other => panic!("both or neither: {:?}", other),
            }
            // Drop the winner's value. Refcount: 2 → 1 (counter still alive).
            drop(popped);
            drop(stolen);
            // counter is the sole owner. No crash = no double free.
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
                if d.push(pushed) { pushed += 1; }
            }
            if d.pop().is_some() {
                stolen.fetch_add(1, Ordering::Relaxed);
            }
        }
        stealer.join().unwrap();
    }
}
