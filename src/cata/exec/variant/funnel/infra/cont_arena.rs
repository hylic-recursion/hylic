//! ContArena<T>: growable bump-allocated slab for parent continuations.
//!
//! Backed by SegmentedSlab: lazily-allocated 64-slot segments.
//! Cont::Direct stores its parent continuation by ContIdx instead of Box.
//! Eliminates the Box<Cont> allocation on every single-child node.
//!
//! Unlike Arena, ContArena uses `take()` (move-out) instead of `get()`
//! (shared reference). Every alloc'd slot is take()n exactly once during
//! the fire_cont cascade. Drop frees segment memory but does not drop
//! values (they were already moved out).

use super::segmented_slab::SegmentedSlab;

/// Index into a ContArena. Copy, no refcount.
#[derive(Clone, Copy)]
pub struct ContIdx(pub u32);

pub(crate) struct ContArena<T>(SegmentedSlab<T>);

unsafe impl<T: Send> Send for ContArena<T> {}
unsafe impl<T: Send + Sync> Sync for ContArena<T> {}

impl<T> ContArena<T> {
    pub fn new() -> Self {
        ContArena(SegmentedSlab::new())
    }

    #[inline]
    pub fn alloc(&self, value: T) -> ContIdx {
        ContIdx(self.0.alloc(value))
    }

    /// Take the value out of the slot (moved out, slot becomes uninit).
    ///
    /// # Safety
    /// Must be called exactly once per allocated slot.
    #[inline]
    pub unsafe fn take(&self, idx: ContIdx) -> T {
        unsafe { self.0.take(idx.0) }
    }
}

impl<T> Drop for ContArena<T> {
    fn drop(&mut self) {
        // Values: every alloc'd cont is take()n exactly once during
        // fire_cont cascade, so all slots are uninit at drop time.
        // If the fold panics mid-execution, some conts may not be taken.
        // For now: the fold doesn't panic (no catch_unwind, panics propagate).
        //
        // Segments: free the heap-allocated segment memory.
        self.0.drop_segments();
    }
}
