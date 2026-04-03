//! Slot<T>: a single storage position in the StealQueue.
//!
//! Four states (AtomicU8):
//!   EMPTY (0)     → not yet written (initial)
//!   AVAILABLE (1) → value written, stealable
//!   STOLEN (2)    → a worker claimed it (worker will dereference the value)
//!   RECLAIMED (3) → the publisher took it back (no worker will touch it)
//!
//! The ownership race is AVAILABLE → {STOLEN | RECLAIMED} via CAS.
//! Only one CAS can win. This resolves who may dereference the value.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU8, Ordering};

pub const EMPTY: u8 = 0;
pub const AVAILABLE: u8 = 1;
pub const STOLEN: u8 = 2;
pub const RECLAIMED: u8 = 3;

pub struct Slot<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    state: AtomicU8,
}

unsafe impl<T: Send> Send for Slot<T> {}
unsafe impl<T: Send> Sync for Slot<T> {}

impl<T> Slot<T> {
    pub fn new() -> Self {
        Slot {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            state: AtomicU8::new(EMPTY),
        }
    }

    /// Write a value and mark AVAILABLE.
    ///
    /// # Safety
    /// Caller must own this position exclusively (via fetch_add on bottom).
    pub unsafe fn write(&self, value: T) {
        unsafe { (*self.value.get()).write(value); }
        self.state.store(AVAILABLE, Ordering::Release);
    }

    /// Try to steal: CAS AVAILABLE → STOLEN.
    /// If won, caller has exclusive right to read the value.
    pub fn try_steal(&self) -> bool {
        self.state
            .compare_exchange(AVAILABLE, STOLEN, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    /// Try to reclaim: CAS AVAILABLE → RECLAIMED.
    /// If won, no worker will ever touch the value. Publisher can drop the frame.
    pub fn try_reclaim(&self) -> bool {
        self.state
            .compare_exchange(AVAILABLE, RECLAIMED, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    /// Read the value out of a stolen slot.
    ///
    /// # Safety
    /// Must only be called after a successful `try_steal()`.
    pub unsafe fn read(&self) -> T {
        unsafe { (*self.value.get()).assume_init_read() }
    }

    /// Load the raw state byte.
    pub fn state(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }
}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        let s = *self.state.get_mut();
        // AVAILABLE: written but never consumed — drop the value.
        // RECLAIMED: publisher took it back — the value was read by
        //   the publisher via TaskRef, which moved it out. Actually,
        //   with RECLAIMED the publisher just "blocked" access — the
        //   value is still in the slot. But the value is a TaskRef
        //   (raw pointers), which is Copy and has no destructor.
        //   For generic T: if RECLAIMED, the value was written but
        //   never read — drop it.
        if s == AVAILABLE || s == RECLAIMED {
            unsafe { (*self.value.get()).assume_init_drop(); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn write_steal_read() {
        let slot = Slot::new();
        unsafe { slot.write(42); }
        assert!(slot.try_steal());
        assert_eq!(unsafe { slot.read() }, 42);
    }

    #[test]
    fn write_reclaim() {
        let slot = Slot::new();
        unsafe { slot.write(99); }
        assert!(slot.try_reclaim());
        // Value is in the slot but RECLAIMED — nobody reads it. Dropped on slot drop.
    }

    #[test]
    fn steal_vs_reclaim_race() {
        for _ in 0..200 {
            let slot = Arc::new(Slot::new());
            unsafe { slot.write(7); }

            let barrier = Arc::new(Barrier::new(2));
            let s2 = slot.clone();
            let b2 = barrier.clone();

            let t = std::thread::spawn(move || {
                b2.wait();
                s2.try_steal()
            });

            barrier.wait();
            let reclaimed = slot.try_reclaim();
            let stolen = t.join().unwrap();

            // Exactly one wins
            assert_ne!(reclaimed, stolen, "both or neither won");
            if stolen {
                assert_eq!(unsafe { slot.read() }, 7);
            }
        }
    }

    #[test]
    fn double_steal_fails() {
        let slot = Slot::new();
        unsafe { slot.write(1); }
        assert!(slot.try_steal());
        assert!(!slot.try_steal());
        assert!(!slot.try_reclaim());
    }

    #[test]
    fn double_reclaim_fails() {
        let slot = Slot::new();
        unsafe { slot.write(1); }
        assert!(slot.try_reclaim());
        assert!(!slot.try_reclaim());
        assert!(!slot.try_steal());
    }

    #[test]
    fn empty_slot_not_stealable() {
        let slot: Slot<i32> = Slot::new();
        assert!(!slot.try_steal());
        assert!(!slot.try_reclaim());
    }
}
