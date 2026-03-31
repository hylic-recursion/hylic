//! WorkPool: fixed-size thread pool with scoped lifecycle.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Configuration for creating a WorkPool.
pub struct WorkPoolSpec {
    pub n_workers: usize,
}

impl WorkPoolSpec {
    pub fn threads(n: usize) -> Self {
        WorkPoolSpec { n_workers: n }
    }
}

/// Fixed-size thread pool for fork-join parallelism.
/// No public constructor — use `WorkPool::with` for scoped access.
pub struct WorkPool {
    queue: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
    condvar: Condvar,
    shutdown: AtomicBool,
}

impl WorkPool {
    /// Create a pool, run `f` with access to it, shut down, join all workers.
    /// The pool cannot escape the closure. Workers are scoped threads —
    /// guaranteed joined on return, even on panic.
    pub fn with<R>(spec: WorkPoolSpec, f: impl FnOnce(&Arc<Self>) -> R) -> R {
        let pool = Arc::new(WorkPool {
            queue: Mutex::new(Vec::new()),
            condvar: Condvar::new(),
            shutdown: AtomicBool::new(false),
        });
        std::thread::scope(|s| {
            for _ in 0..spec.n_workers {
                s.spawn(|| pool.worker_loop());
            }
            let result = f(&pool);
            pool.shutdown();
            result
        })
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.condvar.notify_all();
    }

    fn worker_loop(&self) {
        loop {
            let item = {
                let mut q = self.queue.lock().unwrap();
                loop {
                    if self.shutdown.load(Ordering::Acquire) { return; }
                    if let Some(item) = q.pop() { break item; }
                    q = self.condvar.wait(q).unwrap();
                }
            };
            item();
        }
    }

    pub(super) fn submit(&self, f: Box<dyn FnOnce() + Send>) {
        self.queue.lock().unwrap().push(f);
        self.condvar.notify_one();
    }

    pub(super) fn try_run_one(&self) -> bool {
        let item = self.queue.lock().unwrap().pop();
        match item {
            Some(f) => { f(); true }
            None => false,
        }
    }
}
