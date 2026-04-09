//! Thread pool for the funnel executor.
//!
//! Two modes:
//! - `Pool::with(n, |pool| ...)` — scoped pool via `thread::scope`.
//!   Threads join when the closure returns.
//! - `Pool::global()` — lazy singleton, threads live until process exit.
//!
//! Per-fold memory (arenas, stores) is NOT pre-allocated. Only threads
//! are shared across folds. Each `run_fold` allocates its own typed state.
//!
//! Fold dispatch is serialized per pool (one fold at a time). Workers
//! run in parallel within each fold. See public-surface/07-pool-concurrency.md.

use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use super::infra::eventcount::EventCount;

// ── Job (type-erased, stack-local) ───────────────────

// ANCHOR: job_struct
#[repr(C)]
pub(crate) struct Job {
    pub call: unsafe fn(*const (), usize),
    pub data: *const (),
}
// ANCHOR_END: job_struct

unsafe impl Send for Job {}
unsafe impl Sync for Job {}

// ── PoolState ────────────────────────────────────────

// ANCHOR: pool_state
pub(crate) struct PoolState {
    pub shutdown: AtomicBool,
    pub job_ptr: AtomicPtr<()>,
    pub wake: EventCount,
    /// Threads currently between loading job_ptr and returning from
    /// the job call. dispatch waits for this to reach 0 before returning.
    pub in_job: AtomicU32,
    pub n_threads: usize,
    pub dispatch_lock: Mutex<()>,
}
// ANCHOR_END: pool_state

impl PoolState {
    fn new(n_threads: usize) -> Self {
        PoolState {
            shutdown: AtomicBool::new(false),
            job_ptr: AtomicPtr::new(std::ptr::null_mut()),
            wake: EventCount::new(),
            in_job: AtomicU32::new(0),
            n_threads,
            dispatch_lock: Mutex::new(()),
        }
    }
}

// ── Pool handle ──────────────────────────────────────

/// Handle to a funnel thread pool. Workers are pre-spawned and parked.
/// Per-fold memory (arenas, queue stores) is allocated fresh each fold.
pub struct Pool<'scope> {
    pub(crate) state: &'scope PoolState,
}

impl Pool<'_> {
    /// Scoped pool: spawns `n_workers` threads, calls `f`, joins all threads.
    pub fn with<R>(n_workers: usize, f: impl for<'s> FnOnce(&Pool<'s>) -> R) -> R {
        let state = PoolState::new(n_workers);
        std::thread::scope(|s| {
            for i in 0..n_workers {
                let state_ref = &state;
                s.spawn(move || pool_thread(state_ref, i));
            }
            let result = f(&Pool { state: &state });
            state.shutdown.store(true, Ordering::Release);
            state.wake.notify_all();
            result
        })
    }

    /// Lazy global singleton. Threads live until process exit.
    /// Thread count = available parallelism (or 4 as fallback).
    pub fn global() -> &'static Pool<'static> {
        static POOL: OnceLock<Pool<'static>> = OnceLock::new();
        POOL.get_or_init(|| {
            let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let state: &'static PoolState = Box::leak(Box::new(PoolState::new(n)));
            for i in 0..n {
                let s = state;
                std::thread::spawn(move || pool_thread(s, i));
            }
            Pool { state }
        })
    }

    /// Number of worker threads in this pool.
    pub fn n_workers(&self) -> usize { self.state.n_threads }
}

// ANCHOR: dispatch
// CPS lifecycle: publish → body → seal → latch.
// The body just does fold work and returns a result.
// All pool-thread synchronization is dispatch's responsibility.
pub(crate) fn dispatch<R>(state: &PoolState, job: &Job, body: impl FnOnce() -> R) -> R {
    let _guard = state.dispatch_lock.lock().unwrap();

    // Publish: make job visible to workers
    state.job_ptr.store(job as *const Job as *mut (), Ordering::Release);
    state.wake.notify_all();

    // Body: caller participates in the fold
    let result = body();

    // Seal: prevent new workers from entering
    state.job_ptr.store(std::ptr::null_mut(), Ordering::Release);

    // Latch: wait for all workers to leave the job region in pool_thread.
    // in_job brackets the entire load-job_ptr → call-worker_entry → return
    // sequence, so in_job==0 guarantees no thread holds a reference to
    // the stack-local Job or FoldState.
    let mut spins = 0u32;
    while state.in_job.load(Ordering::Acquire) > 0 {
        spins += 1;
        if spins > 5_000_000 {
            panic!("dispatch latch: {} threads still in job region",
                state.in_job.load(Ordering::Relaxed));
        }
        std::hint::spin_loop();
    }

    result
}
// ANCHOR_END: dispatch

// ANCHOR: pool_thread
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
        // in_job MUST be incremented BEFORE loading job_ptr.
        // This closes the TOCTOU gap: the body cannot return (destroying
        // the Job/FoldState on the stack) while any thread is between
        // loading job_ptr and finishing the job call.
        state.in_job.fetch_add(1, Ordering::Acquire);
        let ptr = state.job_ptr.load(Ordering::Acquire);
        if !ptr.is_null() {
            let job = unsafe { &*(ptr as *const Job) };
            unsafe { (job.call)(job.data, thread_idx); }
        }
        state.in_job.fetch_sub(1, Ordering::Release);
    }
}
// ANCHOR_END: pool_thread
