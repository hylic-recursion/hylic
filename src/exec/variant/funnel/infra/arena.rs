//! Arena<T>: growable bump-allocated slab for ChainNodes.
//!
//! Backed by SegmentedSlab: lazily-allocated 64-slot segments.
//! `alloc(value)` writes to the next slot and returns an `ArenaIdx`
//! (u32, Copy). No refcounting. References are stable — new segment
//! allocations never invalidate existing references.
//!
//! Safety: the arena's lifetime must encompass all references to its slots.
//! In the funnel, the arena is owned by `run_fold` which blocks until the
//! fold completes — no slot outlives the arena.

use super::segmented_slab::SegmentedSlab;

/// Index into an Arena. Copy, no refcount.
#[derive(Clone, Copy)]
pub struct ArenaIdx(u32);

pub(crate) struct Arena<T>(SegmentedSlab<T>);

unsafe impl<T: Send> Send for Arena<T> {}
unsafe impl<T: Send + Sync> Sync for Arena<T> {}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Arena(SegmentedSlab::new())
    }

    #[inline]
    pub fn alloc(&self, value: T) -> ArenaIdx {
        ArenaIdx(self.0.alloc(value))
    }

    /// Get a shared reference to the value at `idx`.
    ///
    /// # Safety
    /// The slot must have been previously allocated via `alloc`.
    #[inline]
    pub unsafe fn get(&self, idx: ArenaIdx) -> &T {
        unsafe { self.0.get_ref(idx.0) }
    }
}

impl<T> Drop for Arena<T> {
    fn drop(&mut self) {
        self.0.drop_allocated_values();
        self.0.drop_segments();
    }
}

#[cfg(test)]
mod tests {
    // SAFETY (throughout this module): every `get` reads an index
    // returned by a prior `alloc` on the same arena.
    use super::*;

    #[test]
    fn basic_alloc_and_get() {
        let arena = Arena::new();
        let i0 = arena.alloc(42);
        let i1 = arena.alloc(99);
        assert_eq!(unsafe { *arena.get(i0) }, 42);
        assert_eq!(unsafe { *arena.get(i1) }, 99);
    }

    #[test]
    fn concurrent_alloc() {
        let arena = std::sync::Arc::new(Arena::new());
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
        for (t, indices) in all.iter().enumerate() {
            for (i, &idx) in indices.iter().enumerate() {
                assert_eq!(unsafe { *arena.get(idx) }, t * 1000 + i);
            }
        }
    }

    #[test]
    fn grows_beyond_initial_segment() {
        let arena = Arena::new();
        let mut indices = Vec::new();
        // 256 elements = 4 segments
        for i in 0..256u32 {
            indices.push(arena.alloc(i));
        }
        for (i, &idx) in indices.iter().enumerate() {
            assert_eq!(unsafe { *arena.get(idx) }, i as u32);
        }
    }

    #[test]
    fn stable_references_across_growth() {
        let arena = Arena::new();
        let i0 = arena.alloc(42u64);
        let r0 = unsafe { arena.get(i0) };
        // Force new segment allocations
        for i in 1..200 {
            arena.alloc(i);
        }
        assert_eq!(*r0, 42);
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
            let arena = Arena::new();
            arena.alloc(Dropper);
            arena.alloc(Dropper);
            arena.alloc(Dropper);
        }
        assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 3);
    }
}
