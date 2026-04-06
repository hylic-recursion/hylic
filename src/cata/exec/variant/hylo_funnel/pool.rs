//! FunnelPool: persistent thread pool.
//!
//! Provides generic threads and a thin job slot (AtomicPtr to a
//! stack-local Job struct). Knows nothing about folds, deques, or tasks.
//! Pool threads park on an eventcount between dispatches.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::sync::Arc;

use super::eventcount::EventCount;

pub const DEQUE_CAPACITY: usize = 4096;

// ── Job (type-erased, stack-local) ───────────────────

#[repr(C)]
pub(crate) struct Job {
    pub call: unsafe fn(*const (), usize),
    pub data: *const (),
}

unsafe impl Send for Job {}
unsafe impl Sync for Job {}

// ── FunnelPool ───────────────────────────────────────

pub struct FunnelPool {
    inner: Arc<PoolInner>,
    _threads: Vec<std::thread::JoinHandle<()>>,
}

pub(crate) struct PoolInner {
    pub shutdown: AtomicBool,
    pub job_ptr: AtomicPtr<()>,
    pub in_job: AtomicU32,
    pub wake: EventCount,
    pub n_threads: usize,
}

impl FunnelPool {
    pub fn new(n_threads: usize) -> Self {
        let inner = Arc::new(PoolInner {
            shutdown: AtomicBool::new(false),
            job_ptr: AtomicPtr::new(std::ptr::null_mut()),
            in_job: AtomicU32::new(0),
            wake: EventCount::new(),
            n_threads,
        });
        let threads: Vec<_> = (0..n_threads).map(|i| {
            let inner = inner.clone();
            std::thread::spawn(move || pool_thread(&inner, i))
        }).collect();
        FunnelPool { inner, _threads: threads }
    }

    pub fn n_threads(&self) -> usize { self.inner.n_threads }
    pub(crate) fn inner(&self) -> &Arc<PoolInner> { &self.inner }

    pub(crate) fn dispatch<R>(&self, job: &Job, body: impl FnOnce() -> R) -> R {
        self.inner.job_ptr.store(job as *const Job as *mut (), Ordering::Release);
        self.inner.wake.notify_all();
        body()
    }
}

impl Drop for FunnelPool {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.inner.wake.notify_all();
        for t in self._threads.drain(..) {
            if let Err(e) = t.join() {
                std::panic::resume_unwind(e);
            }
        }
    }
}

// ── Pool thread ──────────────────────────────────────

fn pool_thread(inner: &PoolInner, thread_idx: usize) {
    let mut last_epoch = 0u32;
    loop {
        loop {
            let token = inner.wake.prepare();
            if inner.shutdown.load(Ordering::Acquire) { return; }
            if token.epoch() > last_epoch {
                last_epoch = token.epoch();
                break;
            }
            inner.wake.wait(token);
        }
        inner.in_job.fetch_add(1, Ordering::Relaxed);
        let ptr = inner.job_ptr.load(Ordering::Acquire);
        if !ptr.is_null() {
            let job = unsafe { &*(ptr as *const Job) };
            unsafe { (job.call)(job.data, thread_idx); }
        }
        inner.in_job.fetch_sub(1, Ordering::Release);
    }
}

// Pool dispatch + latch tested by funnel integration stress tests:
// stress_1500_runs, stress_1500_runs_adjacency, pool_lifecycle_1500.
