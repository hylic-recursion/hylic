//! Arena<T>: bump-allocated slab for ChainNodes.
//!
//! Pre-allocates a fixed-capacity slab. `alloc(value)` writes to the next
//! slot and returns an `ArenaIdx` (u32, Copy). No refcounting. The arena
//! is freed in bulk when dropped.
//!
//! Safety: the arena's lifetime must encompass all references to its slots.
//! In the funnel, the arena is owned by `run_fold` which blocks until the
//! fold completes — no slot outlives the arena.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU32, Ordering};

/// Index into an Arena. Copy, no refcount.
#[derive(Clone, Copy)]
pub struct ArenaIdx(u32);

pub struct Arena<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    next: AtomicU32,
    capacity: u32,
}

unsafe impl<T: Send> Send for Arena<T> {}
unsafe impl<T: Send + Sync> Sync for Arena<T> {}

impl<T> Arena<T> {
    pub fn new(capacity: usize) -> Self {
        let slots: Vec<UnsafeCell<MaybeUninit<T>>> =
            (0..capacity).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
        Arena {
            slots: slots.into_boxed_slice(),
            next: AtomicU32::new(0),
            capacity: capacity as u32,
        }
    }


    pub fn alloc(&self, value: T) -> ArenaIdx {
        let idx = self.next.fetch_add(1, Ordering::Relaxed);
        assert!(idx < self.capacity, "arena exhausted: capacity={}, requested={}", self.capacity, idx + 1);
        unsafe { (*self.slots[idx as usize].get()).write(value); }
        ArenaIdx(idx)
    }

    /// Get a shared reference to the value at `idx`.
    ///
    /// # Safety
    /// The slot must have been previously allocated via `alloc`.
    pub unsafe fn get(&self, idx: ArenaIdx) -> &T {
        unsafe { (*self.slots[idx.0 as usize].get()).assume_init_ref() }
    }

}

impl<T> Drop for Arena<T> {
    fn drop(&mut self) {
        let count = *self.next.get_mut();
        for i in 0..count.min(self.capacity) {
            unsafe { (*self.slots[i as usize].get()).assume_init_drop(); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_alloc_and_get() {
        let arena = Arena::new(8);
        let i0 = arena.alloc(42);
        let i1 = arena.alloc(99);
        assert_eq!(unsafe { *arena.get(i0) }, 42);
        assert_eq!(unsafe { *arena.get(i1) }, 99);
    }

    #[test]
    fn concurrent_alloc() {
        let arena = std::sync::Arc::new(Arena::new(1000));
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let handles: Vec<_> = (0..4).map(|t| {
            let a = arena.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                let mut indices = Vec::new();
                for i in 0..250 {
                    indices.push(a.alloc(t * 1000 + i));
                }
                indices
            })
        }).collect();
        let all: Vec<Vec<ArenaIdx>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        // Verify all values readable
        for (t, indices) in all.iter().enumerate() {
            for (i, &idx) in indices.iter().enumerate() {
                assert_eq!(unsafe { *arena.get(idx) }, t * 1000 + i);
            }
        }
    }

    #[test]
    #[should_panic(expected = "arena exhausted")]
    fn overflow_panics() {
        let arena = Arena::new(2);
        arena.alloc(1);
        arena.alloc(2);
        arena.alloc(3); // panic
    }

    #[test]
    fn drop_runs() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
        struct Dropper;
        impl Drop for Dropper {
            fn drop(&mut self) { DROP_COUNT.fetch_add(1, Ordering::Relaxed); }
        }
        DROP_COUNT.store(0, Ordering::Relaxed);
        {
            let arena = Arena::new(5);
            arena.alloc(Dropper);
            arena.alloc(Dropper);
            arena.alloc(Dropper);
        }
        assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 3);
    }
}
