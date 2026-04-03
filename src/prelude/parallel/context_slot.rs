//! ContextSlot<T>: typed scoped runtime injection.
//!
//! Empty at construction, filled for the duration of a scoped block,
//! automatically cleared when the block ends. Closures capture
//! `&ContextSlot<T>` and call `.get()` to access the injected value.
//!
//! No Copy bound — get() returns &T. The reference is valid for the
//! duration of the scoped block.

use std::cell::UnsafeCell;

/// Typed scoped runtime injection slot.
pub struct ContextSlot<T> {
    inner: UnsafeCell<Option<T>>,
}

// SAFETY: Reads (get) happen during the scoped block, after set, before clear.
// Writes (set/clear) happen on one thread, paired by construction.
unsafe impl<T: Send> Send for ContextSlot<T> {}
unsafe impl<T: Send + Sync> Sync for ContextSlot<T> {}

impl<T> ContextSlot<T> {
    pub const fn new() -> Self {
        ContextSlot { inner: UnsafeCell::new(None) }
    }

    /// Fill the slot, run the body, restore previous value.
    pub fn scoped<R>(&self, value: T, body: impl FnOnce() -> R) -> R {
        let prev = unsafe { (*self.inner.get()).take() };
        unsafe { *self.inner.get() = Some(value); }
        struct RestoreGuard<'a, T>(&'a ContextSlot<T>, Option<T>);
        impl<T> Drop for RestoreGuard<'_, T> {
            fn drop(&mut self) {
                unsafe { *self.0.inner.get() = self.1.take(); }
            }
        }
        let _guard = RestoreGuard(self, prev);
        body()
    }

    /// Read the injected value by reference. Panics if not in a scoped block.
    #[inline]
    pub fn get(&self) -> &T {
        unsafe {
            (*self.inner.get()).as_ref()
                .expect("ContextSlot::get() called outside scoped() block")
        }
    }

    /// Read the injected value, or None.
    #[inline]
    pub fn try_get(&self) -> Option<&T> {
        unsafe { (*self.inner.get()).as_ref() }
    }

    /// Raw access for cases where fill and clear happen in different
    /// closures (e.g., ParEager's lift_fold fills, unwrap clears).
    ///
    /// # Safety
    /// Caller must ensure: set before any get(), clear after all get()s.
    pub unsafe fn inner_raw(&self) -> *mut Option<T> {
        self.inner.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn basic_scoped() {
        let slot = ContextSlot::new();
        assert!(slot.try_get().is_none());
        let result = slot.scoped(42, || {
            assert_eq!(*slot.get(), 42);
            *slot.get() + 1
        });
        assert_eq!(result, 43);
        assert!(slot.try_get().is_none());
    }

    #[test]
    #[should_panic(expected = "outside scoped()")]
    fn get_outside_scoped_panics() {
        let slot: ContextSlot<i32> = ContextSlot::new();
        slot.get();
    }

    #[test]
    fn clear_on_panic() {
        let slot = ContextSlot::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            slot.scoped(99, || { panic!("boom"); });
        }));
        assert!(result.is_err());
        assert!(slot.try_get().is_none());
    }

    #[test]
    fn concurrent_read_during_scoped() {
        let slot = Arc::new(ContextSlot::new());
        let barrier = Arc::new(Barrier::new(5));
        let handles: Vec<_> = (0..4).map(|_| {
            let s = slot.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                *s.get()
            })
        }).collect();
        slot.scoped(777, || {
            barrier.wait();
            let results: Vec<i32> = handles.into_iter()
                .map(|h| h.join().unwrap())
                .collect();
            assert_eq!(results, vec![777; 4]);
        });
    }

    #[test]
    fn nested_scoped() {
        let slot = ContextSlot::new();
        slot.scoped(1, || {
            assert_eq!(*slot.get(), 1);
            slot.scoped(2, || {
                assert_eq!(*slot.get(), 2);
            });
            assert_eq!(*slot.get(), 1);
        });
        assert!(slot.try_get().is_none());
    }

    #[test]
    fn non_copy_type() {
        let slot = ContextSlot::new();
        slot.scoped(String::from("hello"), || {
            assert_eq!(slot.get(), "hello");
        });
        assert!(slot.try_get().is_none());
    }
}
