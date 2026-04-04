//! TaskSlot<F, R>: a fork point on the caller's stack.
//!
//! Holds the f2 closure, a result slot, and a `done` flag.
//!
//! The ownership race is NOT here — it's on the StealQueue slot
//! (AVAILABLE → STOLEN vs AVAILABLE → RECLAIMED). By the time
//! anyone touches the TaskSlot, ownership has been decided:
//! - Publisher reclaimed (queue slot RECLAIMED): only publisher touches TaskSlot.
//! - Worker stole (queue slot STOLEN): only worker touches TaskSlot.
//!   Publisher waits for `done` before returning.
//!
//! This separation means execute_fn doesn't need a CAS — if called,
//! the caller already won the queue slot race.

use std::cell::UnsafeCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};

use super::unsafe_core::task_ref::TaskRef;

pub struct TaskSlot<F, R> {
    func: UnsafeCell<Option<F>>,
    result: UnsafeCell<Option<Result<R, Box<dyn std::any::Any + Send>>>>,
    done: AtomicBool,
}

unsafe impl<F: Send, R: Send> Send for TaskSlot<F, R> {}
unsafe impl<F: Send, R: Send> Sync for TaskSlot<F, R> {}

impl<F: FnOnce() -> R + Send, R: Send> TaskSlot<F, R> {
    pub fn new(func: F) -> Self {
        TaskSlot {
            func: UnsafeCell::new(Some(func)),
            result: UnsafeCell::new(None),
            done: AtomicBool::new(false),
        }
    }

    /// Create a TaskRef pointing to this slot.
    ///
    /// # Safety
    /// The caller must ensure this TaskSlot lives until `is_done()`
    /// returns true. In practice: join() blocks until done.
    pub fn as_task_ref(&self) -> TaskRef {
        unsafe {
            TaskRef::new(self as *const _ as *const (), Self::execute_fn)
        }
    }

    /// Run f2 locally (publisher path, after winning the queue slot race).
    pub fn run_locally(&self) {
        let func = unsafe { (*self.func.get()).take().unwrap() };
        let result = catch_unwind(AssertUnwindSafe(func));
        unsafe { *self.result.get() = Some(result); }
        self.done.store(true, Ordering::Release);
    }

    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    /// Take the result. Must only be called after `is_done()` returns true.
    pub fn take_result(&self) -> R {
        let result = unsafe { (*self.result.get()).take().unwrap() };
        match result {
            Ok(val) => val,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Execute function called by workers via TaskRef.
    /// Called ONLY after the worker won the queue slot CAS (STOLEN).
    /// No ownership CAS needed here — the queue slot race already decided.
    ///
    /// # Safety
    /// `data` must point to a live TaskSlot<F, R>. Guaranteed because
    /// the publisher waits for `done` before returning (stack frame alive).
    unsafe fn execute_fn(data: *const ()) {
        unsafe {
            let slot = &*(data as *const Self);
            let func = (*slot.func.get()).take().unwrap();
            let result = catch_unwind(AssertUnwindSafe(func));
            *slot.result.get() = Some(result);
            slot.done.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_locally() {
        let slot = TaskSlot::new(|| 42);
        slot.run_locally();
        assert!(slot.is_done());
        assert_eq!(slot.take_result(), 42);
    }

    #[test]
    fn execute_via_task_ref() {
        let slot = TaskSlot::new(|| 99);
        let task_ref = slot.as_task_ref();
        unsafe { task_ref.execute(); }
        assert!(slot.is_done());
        assert_eq!(slot.take_result(), 99);
    }

    #[test]
    fn panic_captured() {
        let slot = TaskSlot::new(|| -> i32 { panic!("boom") });
        slot.run_locally();
        assert!(slot.is_done());
        let r = std::panic::catch_unwind(AssertUnwindSafe(|| slot.take_result()));
        assert!(r.is_err());
    }
}
