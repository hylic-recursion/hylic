//! TaskRef: type-erased pointer to a stack-allocated task.
//!
//! A TaskRef is two words: a data pointer + a monomorphized execute
//! function. The data pointer points to a TaskSlot on the caller's
//! stack frame. The execute function knows the concrete type and
//! can cast back to call the stored closure.
//!
//! This is the ONLY raw function pointer in the system.
//!
//! SAFETY: The TaskSlot lives on the caller's stack. join() blocks
//! until the task is resolved (reclaimed or done). The stack frame
//! outlives all uses of the TaskRef. This is the same safety argument
//! as rayon's StackJob pattern.


/// Type-erased handle to a task on someone's stack.
///
/// Two words: data pointer + monomorphized execute function.
/// The execute function encodes the concrete closure type —
/// it casts `data` back to the concrete TaskSlot and runs it.
pub struct TaskRef {
    data: *const (),
    execute: unsafe fn(*const ()),
}

// SAFETY: TaskRef is a pair of pointers. The pointee (TaskSlot)
// is on a stack frame that outlives all uses (join blocks until
// resolution). Send is required for the StealQueue to hold TaskRefs
// that workers on different threads can execute.
unsafe impl Send for TaskRef {}

impl TaskRef {
    /// Create a TaskRef from a raw data pointer and execute function.
    ///
    /// # Safety
    /// The data pointer must remain valid until the TaskRef is either
    /// executed or discarded. The execute function must correctly
    /// handle the data pointer (cast it to the right type, run the
    /// closure, write the result, set the done flag).
    pub unsafe fn new(data: *const (), execute: unsafe fn(*const ())) -> Self {
        TaskRef { data, execute }
    }

    /// Execute this task. Called by workers when they successfully
    /// claim the task's stolen flag.
    ///
    /// # Safety
    /// The data pointer must still be valid (the TaskSlot's stack
    /// frame must be alive). Must only be called once.
    pub unsafe fn execute(self) {
        unsafe { (self.execute)(self.data); }
    }

    /// Create a TaskRef from a boxed closure (for fire-and-forget
    /// tasks like ParEager's leaf finalize submissions).
    ///
    /// The closure is double-boxed: Box<Box<dyn FnOnce() + Send>>
    /// to get a thin pointer for the data field.
    pub fn from_boxed(f: Box<dyn FnOnce() + Send>) -> Self {
        let wrapper = Box::into_raw(Box::new(f));
        TaskRef {
            data: wrapper as *const (),
            execute: execute_boxed,
        }
    }
}

/// Execute function for boxed closures (from_boxed path).
///
/// # Safety
/// `data` must be a pointer from `Box::into_raw(Box::new(Box<dyn FnOnce() + Send>))`.
unsafe fn execute_boxed(data: *const ()) {
    unsafe {
        let wrapper = Box::from_raw(data as *mut Box<dyn FnOnce() + Send>);
        (*wrapper)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    #[test]
    fn execute_boxed_closure() {
        let result = Arc::new(AtomicI32::new(0));
        let r = result.clone();
        let task = TaskRef::from_boxed(Box::new(move || {
            r.store(42, Ordering::Release);
        }));
        unsafe { task.execute(); }
        assert_eq!(result.load(Ordering::Acquire), 42);
    }

    #[test]
    fn execute_typed_task() {
        // Simulate the TaskSlot pattern: a struct on the stack with
        // a closure and a result slot, pointed to by TaskRef.
        use std::cell::UnsafeCell;

        struct FakeTaskSlot {
            func: UnsafeCell<Option<Box<dyn FnOnce() -> i32 + Send>>>,
            result: UnsafeCell<Option<i32>>,
        }

        unsafe fn run_fake(data: *const ()) {
            unsafe {
                let slot = &*(data as *const FakeTaskSlot);
                let f = (*slot.func.get()).take().unwrap();
                *slot.result.get() = Some(f());
            }
        }

        let slot = FakeTaskSlot {
            func: UnsafeCell::new(Some(Box::new(|| 99))),
            result: UnsafeCell::new(None),
        };

        let task = unsafe {
            TaskRef::new(&slot as *const _ as *const (), run_fake)
        };
        unsafe { task.execute(); }
        assert_eq!(unsafe { *slot.result.get() }, Some(99));
    }

    // No test for panic-in-boxed: panics in worker tasks must propagate.
    // Silent swallowing (catch_unwind) was removed by design.
}
