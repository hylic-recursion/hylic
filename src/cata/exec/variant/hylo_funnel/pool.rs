//! Scoped thread pool for the funnel executor.
//!
//! `with_pool` creates a pool using `thread::scope`. Worker threads
//! borrow pool state from the stack. No Arc, no Drop, no persistent
//! threads. Threads join when the scope exits.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use super::eventcount::EventCount;

// ── Job (type-erased, stack-local) ───────────────────

#[repr(C)]
pub(crate) struct Job {
    pub call: unsafe fn(*const (), usize),
    pub data: *const (),
}

unsafe impl Send for Job {}
unsafe impl Sync for Job {}

// ── PoolState (stack-local, borrowed by workers) ─────

pub(crate) struct PoolState {
    pub shutdown: AtomicBool,
    pub job_ptr: AtomicPtr<()>,
    pub wake: EventCount,
    pub n_threads: usize,
}

// ── Scoped pool ──────────────────────────────────────

pub(crate) fn with_pool<R>(n_workers: usize, f: impl FnOnce(&PoolState) -> R) -> R {
    let state = PoolState {
        shutdown: AtomicBool::new(false),
        job_ptr: AtomicPtr::new(std::ptr::null_mut()),
        wake: EventCount::new(),
        n_threads: n_workers,
    };
    std::thread::scope(|s| {
        for i in 0..n_workers {
            let state_ref = &state;
            s.spawn(move || pool_thread(state_ref, i));
        }
        let result = f(&state);
        state.shutdown.store(true, Ordering::Release);
        state.wake.notify_all();
        result
    })
}

pub(crate) fn dispatch<R>(state: &PoolState, job: &Job, body: impl FnOnce() -> R) -> R {
    state.job_ptr.store(job as *const Job as *mut (), Ordering::Release);
    state.wake.notify_all();
    let result = body();
    state.job_ptr.store(std::ptr::null_mut(), Ordering::Release);
    result
}

// ── Pool thread ──────────────────────────────────────

fn pool_thread(state: &PoolState, thread_idx: usize) {
    let mut last_epoch = 0u32;
    loop {
        loop {
            let token = state.wake.prepare();
            if state.shutdown.load(Ordering::Acquire) { return; }
            if token.epoch() > last_epoch {
                last_epoch = token.epoch();
                break;
            }
            state.wake.wait(token);
        }
        let ptr = state.job_ptr.load(Ordering::Acquire);
        if !ptr.is_null() {
            let job = unsafe { &*(ptr as *const Job) };
            unsafe { (job.call)(job.data, thread_idx); }
        }
    }
}
