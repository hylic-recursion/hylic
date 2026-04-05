//! BoundedMpmc<T>: lock-free bounded multi-producer multi-consumer ring buffer.
//!
//! Vyukov-style: per-slot sequence numbers, power-of-2 capacity.
//! No Mutex, no segment allocation, cache-friendly contiguous array.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

#[repr(align(128))]
struct CachePad<T>(T);

struct Slot<T> {
    seq: AtomicUsize,
    data: UnsafeCell<MaybeUninit<T>>,
}

pub(super) struct BoundedMpmc<T> {
    buffer: Box<[Slot<T>]>,
    mask: usize,
    head: CachePad<AtomicUsize>,
    tail: CachePad<AtomicUsize>,
}

unsafe impl<T: Send> Send for BoundedMpmc<T> {}
unsafe impl<T: Send> Sync for BoundedMpmc<T> {}

impl<T> BoundedMpmc<T> {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        let buffer: Vec<Slot<T>> = (0..cap)
            .map(|i| Slot {
                seq: AtomicUsize::new(i),
                data: UnsafeCell::new(MaybeUninit::uninit()),
            })
            .collect();
        BoundedMpmc {
            buffer: buffer.into_boxed_slice(),
            mask: cap - 1,
            head: CachePad(AtomicUsize::new(0)),
            tail: CachePad(AtomicUsize::new(0)),
        }
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        let item = item;
        loop {
            let tail = self.tail.0.load(Ordering::Relaxed);
            let slot = &self.buffer[tail & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            match (seq as isize).wrapping_sub(tail as isize) {
                0 => {
                    if self.tail.0.compare_exchange_weak(
                        tail, tail + 1, Ordering::Relaxed, Ordering::Relaxed,
                    ).is_ok() {
                        unsafe { (*slot.data.get()).write(item); }
                        slot.seq.store(tail + 1, Ordering::Release);
                        return Ok(());
                    }
                }
                d if d < 0 => return Err(item),  // full
                _ => { std::hint::spin_loop(); }  // slot recycled past us
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.0.load(Ordering::Relaxed);
            let slot = &self.buffer[head & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            match (seq as isize).wrapping_sub((head + 1) as isize) {
                0 => {
                    if self.head.0.compare_exchange_weak(
                        head, head + 1, Ordering::Relaxed, Ordering::Relaxed,
                    ).is_ok() {
                        let item = unsafe { (*slot.data.get()).assume_init_read() };
                        slot.seq.store(head + self.mask + 1, Ordering::Release);
                        return Some(item);
                    }
                }
                d if d < 0 => return None,  // empty
                _ => { std::hint::spin_loop(); }  // producer still writing
            }
        }
    }
}

impl<T> Drop for BoundedMpmc<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::sync::atomic::AtomicUsize as AU;

    #[test]
    fn push_pop_basic() {
        let q = BoundedMpmc::new(8);
        assert!(q.push(1).is_ok());
        assert!(q.push(2).is_ok());
        assert!(q.push(3).is_ok());
        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(3));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn fifo_order() {
        let q = BoundedMpmc::new(64);
        for i in 0..50 { assert!(q.push(i).is_ok()); }
        for i in 0..50 { assert_eq!(q.pop(), Some(i)); }
    }

    #[test]
    fn full_returns_err() {
        let q = BoundedMpmc::new(4);
        for i in 0..4 { assert!(q.push(i).is_ok()); }
        assert!(q.push(99).is_err());
        assert_eq!(q.pop(), Some(0));
        assert!(q.push(99).is_ok());
    }

    #[test]
    fn wraparound() {
        let q = BoundedMpmc::new(4);
        for cycle in 0..20 {
            for i in 0..4 { assert!(q.push(cycle * 4 + i).is_ok()); }
            for i in 0..4 { assert_eq!(q.pop(), Some(cycle * 4 + i)); }
        }
    }

    #[test]
    fn single_producer_single_consumer() {
        let q = Arc::new(BoundedMpmc::new(256));
        let n = 10_000usize;
        let q2 = q.clone();
        let producer = std::thread::spawn(move || {
            for i in 0..n {
                while q2.push(i).is_err() { std::hint::spin_loop(); }
            }
        });
        let mut results = Vec::new();
        while results.len() < n {
            if let Some(v) = q.pop() { results.push(v); }
        }
        producer.join().unwrap();
        for i in 0..n { assert_eq!(results[i], i); }
    }

    #[test]
    fn multi_producer_multi_consumer() {
        let q = Arc::new(BoundedMpmc::new(256));
        let n_prod = 4usize;
        let per_prod = 2000usize;
        let total = n_prod * per_prod;
        let consumed = Arc::new(AU::new(0));
        let barrier = Arc::new(Barrier::new(n_prod + 2));

        let producers: Vec<_> = (0..n_prod).map(|_| {
            let q = q.clone(); let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                for i in 0..per_prod {
                    while q.push(i).is_err() { std::thread::yield_now(); }
                }
            })
        }).collect();

        let consumers: Vec<_> = (0..2).map(|_| {
            let q = q.clone(); let b = barrier.clone(); let c = consumed.clone();
            std::thread::spawn(move || {
                b.wait();
                let mut local = 0usize;
                loop {
                    if let Some(_) = q.pop() {
                        local += 1;
                        if c.fetch_add(1, Ordering::Relaxed) + 1 >= total { return local; }
                    } else if c.load(Ordering::Relaxed) >= total {
                        return local;
                    } else {
                        std::thread::yield_now();
                    }
                }
            })
        }).collect();

        for p in producers { p.join().unwrap(); }
        let totals: Vec<_> = consumers.into_iter().map(|c| c.join().unwrap()).collect();
        assert_eq!(totals.iter().sum::<usize>(), total);
    }
}
