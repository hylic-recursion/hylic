//! FunnelPool: persistent thread pool.
//!
//! Provides generic threads and a thin job slot (AtomicPtr to a
//! stack-local Job struct). Knows nothing about folds, deques, or tasks.
//! Pool threads park on an eventcount between dispatches.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use super::deque::WorkerDeque;
use super::eventcount::EventCount;

pub const DEQUE_CAPACITY: usize = 4096;

// ── Job (type-erased, stack-local) ───────────────────

/// A type-erased job: concrete fn pointer + data pointer.
/// Lives on the executor's stack. Pool threads read it through AtomicPtr.
#[repr(C)]
pub(super) struct Job {
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

pub(super) struct PoolInner {
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
    pub(super) fn inner(&self) -> &Arc<PoolInner> { &self.inner }

    /// Dispatch: store job pointer, wake threads, run body, clear pointer.
    /// The body is responsible for ensuring all threads exit the job
    /// before it returns (via FoldView::wait_for_workers_to_exit).
    pub(super) fn dispatch<R>(&self, job: &Job, body: impl FnOnce() -> R) -> R {
        self.inner.job_ptr.store(job as *const Job as *mut (), Ordering::Release);
        self.inner.wake.notify_all();
        body()
        // job_ptr is cleared by the body (before the latch) — not here.
        // This ensures no worker can enter the job after the latch passes.
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

// ── FoldView (per-fold, stack-local, executor-owned) ─

pub(super) struct FoldView {
    pub pool_inner: Arc<PoolInner>,
    pub fold_done: AtomicBool,
    pub idle_count: AtomicU32,
    pub work_available: AtomicU64,
    pub n_workers: usize,
}

impl FoldView {
    pub fn event(&self) -> &EventCount { &self.pool_inner.wake }

    /// Signal that deque `idx` has work. Call after pushing a task.
    pub fn signal_push(&self, deque_idx: usize) {
        self.work_available.fetch_or(1u64 << deque_idx, Ordering::Relaxed);
        if self.idle_count.load(Ordering::Relaxed) > 0 {
            self.pool_inner.wake.notify_one();
        }
    }
}

// ── Pool thread (generic, knows nothing about folds) ─

fn pool_thread(inner: &PoolInner, thread_idx: usize) {
    let mut last_epoch = 0u32;
    loop {
        // Park until woken
        loop {
            let token = inner.wake.prepare();
            if inner.shutdown.load(Ordering::Acquire) { return; }
            if token.epoch() > last_epoch {
                last_epoch = token.epoch();
                break;
            }
            inner.wake.wait(token);
        }
        // Increment in_job BEFORE loading job_ptr. This ensures dispatch's
        // in_job spin cannot see 0 while we're between load and dereference.
        inner.in_job.fetch_add(1, Ordering::Relaxed);
        let ptr = inner.job_ptr.load(Ordering::Acquire);
        if !ptr.is_null() {
            let job = unsafe { &*(ptr as *const Job) };
            unsafe { (job.call)(job.data, thread_idx); }
        }
        inner.in_job.fetch_sub(1, Ordering::Release);
    }
}

// ── Steal helper ─────────────────────────────────────

pub(super) fn steal_from_others<T>(
    deques: &[WorkerDeque<T>],
    my_idx: usize,
    view: &FoldView,
) -> Option<T> {
    let mut bits = view.work_available.load(Ordering::Relaxed);
    bits &= !(1u64 << my_idx); // don't steal from self
    while bits != 0 {
        let target = bits.trailing_zeros() as usize;
        if let Some(task) = deques[target].steal() {
            return Some(task);
        }
        // Deque was empty — clear its bit and try next
        view.work_available.fetch_and(!(1u64 << target), Ordering::Relaxed);
        bits &= !(1u64 << target);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    fn n_threads() -> usize {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    }

    unsafe fn test_worker_entry(data: *const (), thread_idx: usize) {
        let state = unsafe { &*(data as *const TestState) };
        let my_deque = &state.deques[thread_idx];
        loop {
            if let Some(_) = my_deque.pop() {
                state.counter.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if let Some(_) = steal_from_others(state.deques, thread_idx, state.view) {
                state.counter.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if state.view.fold_done.load(Ordering::Acquire) { return; }
            let token = state.view.event().prepare();
            if state.view.fold_done.load(Ordering::Acquire) { return; }
            state.view.event().wait(token);
        }
    }

    struct TestState<'a> {
        view: &'a FoldView,
        deques: &'a [WorkerDeque<u64>],
        counter: &'a AtomicU64,
    }

    #[test]
    fn dispatch_reuse_500() {
        let nt = n_threads();
        let pool = FunnelPool::new(nt);
        for round in 0..500 {
            let counter = AtomicU64::new(0);
            let deques: Vec<WorkerDeque<u64>> =
                (0..nt + 1).map(|_| WorkerDeque::new(DEQUE_CAPACITY)).collect();
            let view = FoldView {
                pool_inner: pool.inner().clone(),
                fold_done: AtomicBool::new(false),
                idle_count: AtomicU32::new(0),
                work_available: AtomicU64::new(0),
                n_workers: nt,
            };
            let state = TestState {
                view: &view,
                deques: deques.as_slice(),
                counter: &counter,
            };
            let job = Job {
                call: test_worker_entry,
                data: &state as *const TestState as *const (),
            };

            pool.dispatch(&job, || {
                let caller = &deques[nt];
                for v in 0..10u64 { caller.push(v); }
                view.signal_push(nt);
                while counter.load(Ordering::Relaxed) < 10 {
                    if let Some(_) = caller.pop() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    } else {
                        std::hint::spin_loop();
                    }
                }
                view.fold_done.store(true, Ordering::Release);
                view.event().notify_all();
                // dispatch handles waiting for in_job == 0
            });
            assert!(counter.load(Ordering::Relaxed) >= 10, "round {round}");
        }
    }

    #[test]
    fn lifecycle_no_work_500() {
        let nt = n_threads();
        let pool = FunnelPool::new(nt);
        for _ in 0..500 {
            let view = FoldView {
                pool_inner: pool.inner().clone(),
                fold_done: AtomicBool::new(false),
                idle_count: AtomicU32::new(0),
                work_available: AtomicU64::new(0),
                n_workers: nt,
            };

            unsafe fn noop_entry(data: *const (), _thread_idx: usize) {
                let view = unsafe { &*(data as *const FoldView) };
                if view.fold_done.load(Ordering::Acquire) { return; }
                let token = view.event().prepare();
                if view.fold_done.load(Ordering::Acquire) { return; }
                view.event().wait(token);
            }

            let job = Job {
                call: noop_entry,
                data: &view as *const FoldView as *const (),
            };
            pool.dispatch(&job, || {
                view.fold_done.store(true, Ordering::Release);
                view.event().notify_all();
            });
        }
    }
}
