//! FunnelPool: persistent thread pool.
//!
//! The pool provides generic threads and a dispatch mechanism.
//! It knows nothing about folds, deques, or tasks. Pool threads
//! are not "workers" — they become workers when a View (per-fold
//! scope inside the executor) gives them a typed job closure.
//!
//! Dispatch: store a job (Arc<dyn Fn(usize)>), wake threads, run
//! the body on the calling thread, clear the job. No barriers,
//! no completion tracking — that's the View's concern.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use super::deque::WorkerDeque;
use super::eventcount::EventCount;

pub const DEQUE_CAPACITY: usize = 4096;

// ── FunnelPool ───────────────────────────────────────

pub struct FunnelPool {
    inner: Arc<PoolInner>,
    _threads: Vec<std::thread::JoinHandle<()>>,
}

pub(super) struct PoolInner {
    pub shutdown: AtomicBool,
    pub job: std::sync::Mutex<Option<Arc<dyn Fn(usize) + Send + Sync>>>,
    pub wake: EventCount,
    pub n_threads: usize,
}

impl FunnelPool {
    pub fn new(n_threads: usize) -> Self {
        let inner = Arc::new(PoolInner {
            shutdown: AtomicBool::new(false),
            job: std::sync::Mutex::new(None),
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

    /// Dispatch a job to pool threads and run body on the calling thread.
    /// The pool stores the job, wakes threads, runs body, clears the job.
    /// Completion tracking is the caller's responsibility (via the View).
    pub(super) fn dispatch<R>(
        &self,
        job: Arc<dyn Fn(usize) + Send + Sync>,
        body: impl FnOnce() -> R,
    ) -> R {
        *self.inner.job.lock().unwrap() = Some(job);
        self.inner.wake.notify_all();
        let result = body();
        *self.inner.job.lock().unwrap() = None;
        result
    }
}

impl Drop for FunnelPool {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        *self.inner.job.lock().unwrap() = None;
        self.inner.wake.notify_all();
        for t in self._threads.drain(..) {
            if let Err(e) = t.join() {
                std::panic::resume_unwind(e);
            }
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
        // Call whatever job the View published
        let job = inner.job.lock().unwrap().clone();
        if let Some(f) = job {
            f(thread_idx);
        }
    }
}

// ── FoldView shared state (per-fold, stack-local) ────

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

    /// Wait for all pool threads to exit this View's closure.
    /// CPS guarantees they're in the idle path — this should resolve
    /// within microseconds. Panics if it doesn't (structural bug).
    pub fn wait_for_workers_to_exit(&self) {
        let mut spins = 0u32;
        while self.active_in_view.load(Ordering::Acquire) > 0 {
            spins += 1;
            if spins > 5_000_000 {
                panic!(
                    "FoldView teardown: {} workers still active after fold_done. \
                     CPS guarantees all work is complete — this is a bug.",
                    self.active_in_view.load(Ordering::Relaxed),
                );
            }
            std::hint::spin_loop();
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

    /// 500-iteration dispatch reuse stress test.
    #[test]
    fn dispatch_reuse_500() {
        let pool = FunnelPool::new(3);
        let pool_inner = pool.inner().clone();
        for round in 0..500 {
            let counter = Arc::new(AtomicU64::new(0));
            let done = Arc::new(AtomicBool::new(false));
            let active = Arc::new(AtomicU32::new(0));
            let deques: Arc<Vec<WorkerDeque<u64>>> = Arc::new(
                (0..4).map(|_| WorkerDeque::new(DEQUE_CAPACITY)).collect()
            );

            let w_counter = counter.clone();
            let w_done = done.clone();
            let w_active = active.clone();
            let w_deques = deques.clone();
            let w_inner = pool_inner.clone();
            let job: Arc<dyn Fn(usize) + Send + Sync> = Arc::new(move |idx: usize| {
                w_active.fetch_add(1, Ordering::Relaxed);
                let my_deque = &w_deques[idx];
                loop {
                    if let Some(_) = my_deque.pop() {
                        w_counter.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    if let Some(_) = steal_from_others(&w_deques, idx) {
                        w_counter.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    if w_done.load(Ordering::Acquire) {
                        w_active.fetch_sub(1, Ordering::Release);
                        return;
                    }
                    let token = w_inner.wake.prepare();
                    if w_done.load(Ordering::Acquire) {
                        w_active.fetch_sub(1, Ordering::Release);
                        return;
                    }
                    w_inner.wake.wait(token);
                }
            });

            let c = counter.clone();
            pool.dispatch(job, || {
                let caller = &deques[3];
                for v in 0..10u64 { caller.push(v); }
                pool_inner.wake.notify_all();
                while c.load(Ordering::Relaxed) < 10 {
                    if let Some(_) = caller.pop() {
                        c.fetch_add(1, Ordering::Relaxed);
                    } else {
                        std::hint::spin_loop();
                    }
                }
                done.store(true, Ordering::Release);
                pool_inner.wake.notify_all();
                // Wait for workers to exit (like the real FoldView)
                let mut spins = 0u32;
                while active.load(Ordering::Acquire) > 0 {
                    spins += 1;
                    assert!(spins < 5_000_000, "round {round}: workers didn't exit");
                    std::hint::spin_loop();
                }
            });
            assert!(c.load(Ordering::Relaxed) >= 10, "round {round}");
        }
    }

    #[test]
    fn lifecycle_no_work_500() {
        let pool = FunnelPool::new(4);
        let pool_inner = pool.inner().clone();
        let done = Arc::new(AtomicBool::new(false));
        for _ in 0..500 {
            done.store(false, Ordering::Relaxed);
            let w_done = done.clone();
            let w_inner = pool_inner.clone();
            let active = Arc::new(AtomicU32::new(0));
            let w_active = active.clone();
            let job: Arc<dyn Fn(usize) + Send + Sync> = Arc::new(move |_: usize| {
                w_active.fetch_add(1, Ordering::Relaxed);
                if w_done.load(Ordering::Acquire) {
                    w_active.fetch_sub(1, Ordering::Release);
                    return;
                }
                let token = w_inner.wake.prepare();
                if w_done.load(Ordering::Acquire) {
                    w_active.fetch_sub(1, Ordering::Release);
                    return;
                }
                w_inner.wake.wait(token);
                w_active.fetch_sub(1, Ordering::Release);
            });
            pool.dispatch(job, || {
                done.store(true, Ordering::Release);
                pool_inner.wake.notify_all();
                while active.load(Ordering::Acquire) > 0 {
                    std::hint::spin_loop();
                }
            });
        }
    }
}
