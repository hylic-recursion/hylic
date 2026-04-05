//! FunnelPool: persistent thread pool.
//!
//! Provides generic threads and a thin job slot (AtomicPtr to a
//! stack-local Job struct). Knows nothing about folds, deques, or tasks.
//! Pool threads park on an eventcount between dispatches.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
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
    pub wake: EventCount,
    pub n_threads: usize,
}

impl FunnelPool {
    pub fn new(n_threads: usize) -> Self {
        let inner = Arc::new(PoolInner {
            shutdown: AtomicBool::new(false),
            job_ptr: AtomicPtr::new(std::ptr::null_mut()),
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
        let result = body();
        self.inner.job_ptr.store(std::ptr::null_mut(), Ordering::Release);
        result
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
    pub active_in_view: AtomicU32,
    pub n_workers: usize,
}

impl FoldView {
    pub fn event(&self) -> &EventCount { &self.pool_inner.wake }

    pub fn notify_one(&self) {
        if self.idle_count.load(Ordering::Relaxed) > 0 {
            self.pool_inner.wake.notify_one();
        }
    }

    /// Assert all pool threads have exited this View's typed code.
    /// CPS guarantees all work is done — workers are in the idle path.
    /// If they don't exit promptly, it's a structural bug.
    pub fn wait_for_workers_to_exit(&self) {
        let mut spins = 0u32;
        while self.active_in_view.load(Ordering::Acquire) > 0 {
            spins += 1;
            if spins > 5_000_000 {
                panic!(
                    "FoldView teardown: {} workers still active after fold_done",
                    self.active_in_view.load(Ordering::Relaxed),
                );
            }
            std::hint::spin_loop();
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
        // Call the job if set
        let ptr = inner.job_ptr.load(Ordering::Acquire);
        if !ptr.is_null() {
            let job = unsafe { &*(ptr as *const Job) };
            unsafe { (job.call)(job.data, thread_idx); }
        }
    }
}

// ── Steal helper ─────────────────────────────────────

pub(super) fn steal_from_others<T>(deques: &[WorkerDeque<T>], my_idx: usize) -> Option<T> {
    let n = deques.len();
    let start = my_idx.wrapping_add(1);
    for i in 0..n {
        let idx = (start + i) % n;
        if idx == my_idx { continue; }
        if let Some(task) = deques[idx].steal() {
            return Some(task);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    /// Type-erased worker entry for the pool test.
    unsafe fn test_worker_entry(data: *const (), thread_idx: usize) {
        let state = unsafe { &*(data as *const TestState) };
        state.view.active_in_view.fetch_add(1, Ordering::Relaxed);
        let my_deque = &state.deques[thread_idx];
        loop {
            if let Some(_) = my_deque.pop() {
                state.counter.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if let Some(_) = steal_from_others(state.deques, thread_idx) {
                state.counter.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if state.view.fold_done.load(Ordering::Acquire) {
                state.view.active_in_view.fetch_sub(1, Ordering::Release);
                return;
            }
            let token = state.view.event().prepare();
            if state.view.fold_done.load(Ordering::Acquire) {
                state.view.active_in_view.fetch_sub(1, Ordering::Release);
                return;
            }
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
        let pool = FunnelPool::new(3);
        for round in 0..500 {
            let counter = AtomicU64::new(0);
            let deques: Vec<WorkerDeque<u64>> =
                (0..4).map(|_| WorkerDeque::new(DEQUE_CAPACITY)).collect();
            let view = FoldView {
                pool_inner: pool.inner().clone(),
                fold_done: AtomicBool::new(false),
                idle_count: AtomicU32::new(0),
                active_in_view: AtomicU32::new(0),
                n_workers: 3,
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
                let caller = &deques[3];
                for v in 0..10u64 { caller.push(v); }
                view.event().notify_all();
                while counter.load(Ordering::Relaxed) < 10 {
                    if let Some(_) = caller.pop() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    } else {
                        std::hint::spin_loop();
                    }
                }
                view.fold_done.store(true, Ordering::Release);
                view.event().notify_all();
                view.wait_for_workers_to_exit();
            });
            assert!(counter.load(Ordering::Relaxed) >= 10, "round {round}");
        }
    }

    #[test]
    fn lifecycle_no_work_500() {
        let pool = FunnelPool::new(4);
        for _ in 0..500 {
            let view = FoldView {
                pool_inner: pool.inner().clone(),
                fold_done: AtomicBool::new(false),
                idle_count: AtomicU32::new(0),
                active_in_view: AtomicU32::new(0),
                n_workers: 4,
            };

            unsafe fn noop_entry(data: *const (), _thread_idx: usize) {
                let view = unsafe { &*(data as *const FoldView) };
                view.active_in_view.fetch_add(1, Ordering::Relaxed);
                if view.fold_done.load(Ordering::Acquire) {
                    view.active_in_view.fetch_sub(1, Ordering::Release);
                    return;
                }
                let token = view.event().prepare();
                if view.fold_done.load(Ordering::Acquire) {
                    view.active_in_view.fetch_sub(1, Ordering::Release);
                    return;
                }
                view.event().wait(token);
                view.active_in_view.fetch_sub(1, Ordering::Release);
            }

            let job = Job {
                call: noop_entry,
                data: &view as *const FoldView as *const (),
            };
            pool.dispatch(&job, || {
                view.fold_done.store(true, Ordering::Release);
                view.event().notify_all();
                view.wait_for_workers_to_exit();
            });
        }
    }
}
