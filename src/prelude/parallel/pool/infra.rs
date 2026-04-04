//! Pool infrastructure: WorkPool, worker threads, wake signaling.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::super::steal_queue::StealQueue;
use super::super::unsafe_core::task_ref::TaskRef;

// ── WakeSignal ───────────────────────────────────────

pub(crate) struct WakeSignal {
    pub(crate) condvar: Condvar,
    pub(crate) lock: Mutex<()>,
}

impl WakeSignal {
    pub(crate) fn new() -> Self { WakeSignal { condvar: Condvar::new(), lock: Mutex::new(()) } }
    pub(crate) fn wake_one(&self) { self.condvar.notify_one(); }
    pub(crate) fn wake_all(&self) { self.condvar.notify_all(); }
}

// ── WorkPool ─────────────────────────────────────────

pub struct WorkPoolSpec {
    pub n_workers: usize,
}

impl WorkPoolSpec {
    pub fn threads(n: usize) -> Self { WorkPoolSpec { n_workers: n } }
}

pub struct WorkPool {
    pub(crate) views: Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
    pub(crate) signal: Arc<WakeSignal>,
    shutdown: AtomicBool,
}

impl WorkPool {
    pub fn with<R>(spec: WorkPoolSpec, f: impl FnOnce(&Arc<Self>) -> R) -> R {
        let pool = Arc::new(WorkPool {
            views: Arc::new(Mutex::new(Vec::new())),
            signal: Arc::new(WakeSignal::new()),
            shutdown: AtomicBool::new(false),
        });
        std::thread::scope(|s| {
            for _ in 0..spec.n_workers {
                let views = pool.views.clone();
                let signal = pool.signal.clone();
                let shutdown = &pool.shutdown;
                s.spawn(move || worker_loop(&views, &signal, shutdown));
            }
            struct ShutdownGuard<'a>(&'a WorkPool);
            impl Drop for ShutdownGuard<'_> {
                fn drop(&mut self) {
                    // Store shutdown under the condvar mutex to close the
                    // lost-wakeup window between shutdown check and condvar.wait.
                    {
                        let _guard = self.0.signal.lock.lock().unwrap();
                        self.0.shutdown.store(true, Ordering::Release);
                    }
                    self.0.signal.wake_all();
                }
            }
            let _guard = ShutdownGuard(&pool);
            f(&pool)
        })
    }
}

// ── Worker loop ──────────────────────────────────────

fn worker_loop(
    views: &Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
    signal: &Arc<WakeSignal>,
    shutdown: &AtomicBool,
) {
    loop {
        if let Some(task) = steal_from_views(views) {
            unsafe { task.execute(); }
            continue;
        }
        let guard = signal.lock.lock().unwrap();
        if shutdown.load(Ordering::Acquire) { return; }
        if let Some(task) = steal_from_views(views) {
            drop(guard);
            unsafe { task.execute(); }
            continue;
        }
        let _guard = signal.condvar.wait(guard).unwrap();
        if shutdown.load(Ordering::Acquire) { return; }
    }
}

pub(crate) fn steal_from_views(views: &Mutex<Vec<Arc<StealQueue<TaskRef>>>>) -> Option<TaskRef> {
    let views = views.lock().unwrap();
    for deque in views.iter() {
        if let Some(task) = deque.steal() {
            return Some(task);
        }
    }
    None
}
