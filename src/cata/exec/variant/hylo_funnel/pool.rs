//! FunnelPool: persistent thread pool for the hylo-funnel executor.
//!
//! Workers spawn once and park between folds. Each fold stores a
//! type-erased job, bumps a job counter, and signals workers.
//! Workers call the job, signal completion, park until next job.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::sync::Arc;

use super::deque::WorkerDeque;
use super::eventcount::EventCount;

pub const DEQUE_CAPACITY: usize = 4096;

// ── FunnelPool (persistent) ───��──────────────────────

pub struct FunnelPool {
    inner: Arc<PoolInner>,
    _workers: Vec<std::thread::JoinHandle<()>>,
}

struct PoolInner {
    shutdown: AtomicBool,
    /// Type-erased job: `*const &dyn Fn(usize)`.
    job_ptr: AtomicPtr<()>,
    /// Monotonic job counter. Bumped per fold.
    job_epoch: AtomicU32,
    /// Workers decrement when done with current job.
    workers_done: AtomicU32,
    /// Wake workers when new job is ready.
    wake: EventCount,
    n_workers: usize,
}

impl FunnelPool {
    pub fn new(n_workers: usize) -> Self {
        let inner = Arc::new(PoolInner {
            shutdown: AtomicBool::new(false),
            job_ptr: AtomicPtr::new(std::ptr::null_mut()),
            job_epoch: AtomicU32::new(0),
            workers_done: AtomicU32::new(0),
            wake: EventCount::new(),
            n_workers,
        });
        let workers: Vec<_> = (0..n_workers).map(|i| {
            let inner = inner.clone();
            std::thread::spawn(move || persistent_worker(&inner, i))
        }).collect();
        FunnelPool { inner, _workers: workers }
    }

    pub fn n_workers(&self) -> usize { self.inner.n_workers }

    /// Run a typed fold on the persistent pool.
    pub(super) fn run_job<T: Send, R>(
        &self,
        body: impl FnOnce(&FunnelPoolShared, &[WorkerDeque<T>]) -> R,
        worker_fn: impl Fn(&FunnelPoolShared, &[WorkerDeque<T>], usize) + Sync,
    ) -> R {
        let n = self.inner.n_workers;
        let deques: Vec<WorkerDeque<T>> = (0..n + 1)
            .map(|_| WorkerDeque::new(DEQUE_CAPACITY))
            .collect();

        let shared = FunnelPoolShared {
            event: &self.inner.wake,
            fold_done: AtomicBool::new(false),
            idle_count: AtomicU32::new(0),
            n_workers: n,
        };

        // Type-erased job: workers call this with their index.
        let typed_job = |worker_idx: usize| {
            worker_fn(&shared, &deques, worker_idx);
        };
        let job_ref: &dyn Fn(usize) = &typed_job;
        let job_ptr = &job_ref as *const &dyn Fn(usize) as *mut ();

        // Publish job + bump epoch
        self.inner.workers_done.store(0, Ordering::Relaxed);
        self.inner.job_ptr.store(job_ptr, Ordering::Release);
        self.inner.job_epoch.fetch_add(1, Ordering::Release);
        self.inner.wake.notify_all();

        // Run the caller's body (which seeds the walk and helps)
        let result = body(&shared, &deques);

        // Wait for all workers to complete this job
        while self.inner.workers_done.load(Ordering::Acquire) < n as u32 {
            std::hint::spin_loop();
        }

        // Clear job pointer (workers see null, park until next epoch)
        self.inner.job_ptr.store(std::ptr::null_mut(), Ordering::Release);

        result
    }
}

impl Drop for FunnelPool {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.inner.wake.notify_all();
        for w in self._workers.drain(..) {
            let _ = w.join();
        }
    }
}

// ── FunnelPoolShared (per-fold borrowing view) ───────

pub(super) struct FunnelPoolShared<'a> {
    pub event: &'a EventCount,
    pub fold_done: AtomicBool,
    pub idle_count: AtomicU32,
    pub n_workers: usize,
}

impl FunnelPoolShared<'_> {
    pub fn notify_one(&self) {
        if self.idle_count.load(Ordering::Relaxed) > 0 {
            self.event.notify_one();
        }
    }
}

// ── Persistent worker ────────────────────────────────

fn persistent_worker(inner: &PoolInner, worker_idx: usize) {
    let mut last_epoch = 0u32;

    loop {
        // Wait for a new job (epoch > last_epoch) or shutdown
        loop {
            let token = inner.wake.prepare();
            if inner.shutdown.load(Ordering::Acquire) { return; }
            let epoch = inner.job_epoch.load(Ordering::Acquire);
            if epoch > last_epoch {
                last_epoch = epoch;
                break;
            }
            inner.wake.wait(token);
        }

        // Load and call the type-erased job
        let ptr = inner.job_ptr.load(Ordering::Acquire);
        if !ptr.is_null() {
            let job: &dyn Fn(usize) = unsafe { *(ptr as *const &dyn Fn(usize)) };
            job(worker_idx);
        }

        // Signal completion
        inner.workers_done.fetch_add(1, Ordering::Release);
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

    #[test]
    fn persistent_pool_reuse() {
        let pool = FunnelPool::new(3);
        for round in 0..20 {
            let counter = Arc::new(AtomicU64::new(0));
            let c = counter.clone();
            pool.run_job(
                |shared, deques: &[WorkerDeque<u64>]| {
                    let caller = &deques[shared.n_workers];
                    for v in 0..10u64 { caller.push(v); }
                    shared.event.notify_all();
                    while c.load(Ordering::Relaxed) < 10 {
                        if let Some(v) = caller.pop() {
                            c.fetch_add(1, Ordering::Relaxed);
                        } else {
                            std::hint::spin_loop();
                        }
                    }
                },
                |shared, deques, my_idx| {
                    let my_deque = &deques[my_idx];
                    loop {
                        if let Some(_) = my_deque.pop() {
                            counter.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        if let Some(_) = steal_from_others(deques, my_idx) {
                            counter.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        let token = shared.event.prepare();
                        if counter.load(Ordering::Relaxed) >= 10 { return; }
                        shared.event.wait(token);
                        if counter.load(Ordering::Relaxed) >= 10 { return; }
                    }
                },
            );
            assert!(counter.load(Ordering::Relaxed) >= 10, "round {round}");
        }
    }

    #[test]
    fn lifecycle_no_work_500() {
        let pool = FunnelPool::new(4);
        for _ in 0..500 {
            pool.run_job(
                |_, _: &[WorkerDeque<u32>]| {},
                |_, _, _| {},  // worker does nothing, returns immediately
            );
        }
    }
}
