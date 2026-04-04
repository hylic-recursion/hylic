//! TaskRef: type-erased task pointer for the funnel's MPMC queue.
//!
//! from_fn<F> monomorphizes the execute function — no vtable, no double-Box.

pub(super) struct TaskRef {
    data: *const (),
    execute: unsafe fn(*const ()),
}

unsafe impl Send for TaskRef {}

impl TaskRef {
    pub fn from_fn<F: FnOnce() + Send + 'static>(f: F) -> Self {
        let ptr = Box::into_raw(Box::new(f));
        TaskRef { data: ptr as *const (), execute: execute_fn::<F> }
    }

    pub unsafe fn execute(self) {
        unsafe { (self.execute)(self.data); }
    }
}

unsafe fn execute_fn<F: FnOnce() + Send>(data: *const ()) {
    unsafe {
        let f = Box::from_raw(data as *mut F);
        (*f)();
    }
}
