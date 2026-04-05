//! ContArena<H, R>: bump-allocated slab for parent continuations.
//!
//! Cont::Direct stores its parent continuation by ArenaIdx instead of Box.
//! Eliminates the Box<Cont> allocation on every single-child node.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU32, Ordering};

/// Index into a ContArena. Copy, no refcount.
#[derive(Clone, Copy)]
pub struct ContIdx(pub u32);

pub struct ContArena<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    next: AtomicU32,
    capacity: u32,
}

unsafe impl<T: Send> Send for ContArena<T> {}
unsafe impl<T: Send + Sync> Sync for ContArena<T> {}

impl<T> ContArena<T> {
    pub fn new(capacity: usize) -> Self {
        let slots: Vec<UnsafeCell<MaybeUninit<T>>> =
            (0..capacity).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
        ContArena {
            slots: slots.into_boxed_slice(),
            next: AtomicU32::new(0),
            capacity: capacity as u32,
        }
    }

    pub fn alloc(&self, value: T) -> ContIdx {
        let idx = self.next.fetch_add(1, Ordering::Relaxed);
        assert!(idx < self.capacity, "cont arena exhausted: cap={}", self.capacity);
        unsafe { (*self.slots[idx as usize].get()).write(value); }
        ContIdx(idx)
    }

    /// Take the value out of the slot (moved out, slot becomes uninit).
    ///
    /// # Safety
    /// Must be called exactly once per allocated slot.
    pub unsafe fn take(&self, idx: ContIdx) -> T {
        unsafe { (*self.slots[idx.0 as usize].get()).assume_init_read() }
    }
}

impl<T> Drop for ContArena<T> {
    fn drop(&mut self) {
        // Note: slots that were take()n are already uninit.
        // Slots that were alloc'd but never take()n need dropping.
        // We can't distinguish — but in the funnel, every alloc'd cont
        // is take()n exactly once during fire_cont cascade. So all slots
        // are uninit at drop time. No-op.
        //
        // If the fold panics mid-execution, some conts may not be taken.
        // For safety, we'd need a bitset tracking which are live.
        // For now: the fold doesn't panic (no catch_unwind, panics propagate).
    }
}
