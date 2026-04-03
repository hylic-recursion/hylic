//! WorkPool + PoolExecView + ViewHandle: scoped thread pool.
//!
//! join() protocol:
//! 1. Create TaskSlot on stack, push TaskRef to StealQueue → position P
//! 2. Run f1
//! 3. CAS queue slot[P]: AVAILABLE → RECLAIMED (publisher) vs AVAILABLE → STOLEN (worker)
//! 4a. Publisher wins: run f2 locally, return. No worker touches the TaskRef.
//! 4b. Worker wins: worker runs f2 via TaskRef. Publisher waits for done.
//!
//! No dangling pointers: the frame stays alive because either the publisher
//! runs f2 (frame alive), or the publisher waits for the worker (frame alive).

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::steal_queue::StealQueue;
use super::task_slot::TaskSlot;
use super::unsafe_core::task_ref::TaskRef;

// ── SyncRef ──────────────────────────────────────────

pub struct SyncRef<'a, T: ?Sized>(pub &'a T);
unsafe impl<T: ?Sized> Sync for SyncRef<'_, T> {}
unsafe impl<T: ?Sized> Send for SyncRef<'_, T> {}
impl<T: ?Sized> std::ops::Deref for SyncRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.0 }
}

// ── WakeSignal ───────────────────────────────────────

struct WakeSignal {
    condvar: Condvar,
    lock: Mutex<()>,
}

impl WakeSignal {
    fn new() -> Self { WakeSignal { condvar: Condvar::new(), lock: Mutex::new(()) } }
    fn wake_one(&self) { self.condvar.notify_one(); }
    fn wake_all(&self) { self.condvar.notify_all(); }
}

// ── ViewHandle ───────────────────────────────────────

#[derive(Clone)]
pub struct ViewHandle {
    deque: Arc<StealQueue<TaskRef>>,
    signal: Arc<WakeSignal>,
    views: Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
}

impl ViewHandle {
    pub fn submit(&self, f: Box<dyn FnOnce() + Send>) {
        self.deque.push(TaskRef::from_boxed(f));
        self.signal.wake_one();
    }

    pub fn help_once(&self) -> bool {
        if let Some(task_ref) = self.deque.steal() {
            unsafe { task_ref.execute(); }
            return true;
        }
        if let Some(task_ref) = steal_from_views(&self.views) {
            unsafe { task_ref.execute(); }
            return true;
        }
        false
    }
}

// ── WorkPool ─────────────────────────────────────────

pub struct WorkPoolSpec {
    pub n_workers: usize,
}

impl WorkPoolSpec {
    pub fn threads(n: usize) -> Self { WorkPoolSpec { n_workers: n } }
}

pub struct WorkPool {
    views: Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
    signal: Arc<WakeSignal>,
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
                    self.0.shutdown.store(true, Ordering::Release);
                    self.0.signal.wake_all();
                }
            }
            let _guard = ShutdownGuard(&pool);
            f(&pool)
        })
    }
}

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

fn steal_from_views(views: &Mutex<Vec<Arc<StealQueue<TaskRef>>>>) -> Option<TaskRef> {
    let views = views.lock().unwrap();
    for deque in views.iter() {
        if let Some(task) = deque.steal() {
            return Some(task);
        }
    }
    None
}

// ── PoolExecView ─────────────────────────────────────

pub struct PoolExecView {
    deque: Arc<StealQueue<TaskRef>>,
    signal: Arc<WakeSignal>,
    views: Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
}

impl PoolExecView {
    pub fn new(pool: &WorkPool) -> Self {
        let deque = Arc::new(StealQueue::new());
        pool.views.lock().unwrap().push(deque.clone());
        PoolExecView {
            deque,
            signal: pool.signal.clone(),
            views: pool.views.clone(),
        }
    }

    pub fn handle(&self) -> ViewHandle {
        ViewHandle {
            deque: self.deque.clone(),
            signal: self.signal.clone(),
            views: self.views.clone(),
        }
    }

    pub fn join<A: Send, B: Send>(
        &self,
        f1: impl FnOnce() -> A + Send,
        f2: impl FnOnce() -> B + Send,
    ) -> (A, B) {
        let slot = TaskSlot::new(f2);
        let pos = self.deque.push(slot.as_task_ref());
        self.signal.wake_one();

        let result_a = catch_unwind(AssertUnwindSafe(f1));

        // Ownership race: CAS queue slot AVAILABLE → RECLAIMED.
        if self.deque.try_reclaim(pos) {
            // Publisher won. No worker will touch the TaskRef.
            // Safe to run f2 locally and return (frame stays alive).
            slot.run_locally();
        } else {
            // Worker won (STOLEN). Worker is running f2 via TaskRef.
            // Must wait for done — frame must stay alive until worker
            // finishes dereferencing the TaskRef.
            while !slot.is_done() {
                if !self.help_once() {
                    std::hint::spin_loop();
                }
            }
        }

        let b = slot.take_result();
        match result_a {
            Ok(a) => (a, b),
            Err(e) => resume_unwind(e),
        }
    }

    fn help_once(&self) -> bool {
        if let Some(task) = self.deque.steal() {
            unsafe { task.execute(); }
            return true;
        }
        if let Some(task) = steal_from_views(&self.views) {
            unsafe { task.execute(); }
            return true;
        }
        false
    }
}

impl Drop for PoolExecView {
    fn drop(&mut self) {
        let ptr = Arc::as_ptr(&self.deque);
        let mut views = self.views.lock().unwrap();
        if let Some(pos) = views.iter().position(|d| Arc::as_ptr(d) == ptr) {
            views.swap_remove(pos);
        }
    }
}

// ── fork_join_map ────────────────────────────────────

pub fn fork_join_map<T, R: Send, F: Fn(&T) -> R + Send + Sync>(
    view: &PoolExecView,
    items: &[T],
    f: &F,
    depth: usize,
    max_depth: usize,
) -> Vec<R> {
    let items = SyncRef(items);
    fork_join_map_inner(view, &items, f, depth, max_depth)
}

fn fork_join_map_inner<T, R: Send, F: Fn(&T) -> R + Send + Sync>(
    view: &PoolExecView,
    items: &SyncRef<'_, [T]>,
    f: &F,
    depth: usize,
    max_depth: usize,
) -> Vec<R> {
    if items.len() <= 1 || depth >= max_depth {
        return items.iter().map(|x| f(x)).collect();
    }
    let mid = items.len() / 2;
    let left = SyncRef(&items[..mid]);
    let right = SyncRef(&items[mid..]);
    let (l, r) = view.join(
        || fork_join_map_inner(view, &left, f, depth + 1, max_depth),
        || fork_join_map_inner(view, &right, f, depth + 1, max_depth),
    );
    let mut result = l;
    result.extend(r);
    result
}

// ── Tests ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_basic() {
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let view = PoolExecView::new(pool);
            let (a, b) = view.join(|| 1 + 2, || 3 + 4);
            assert_eq!((a, b), (3, 7));
        });
    }

    #[test]
    fn join_nested() {
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let view = PoolExecView::new(pool);
            let (a, b) = view.join(
                || { let (x, y) = view.join(|| 1, || 2); x + y },
                || { let (x, y) = view.join(|| 3, || 4); x + y },
            );
            assert_eq!(a + b, 10);
        });
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn join_propagates_panic() {
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let view = PoolExecView::new(pool);
            view.join(|| 1, || -> i32 { panic!("boom") });
        });
    }

    #[test]
    fn join_zero_workers() {
        WorkPool::with(WorkPoolSpec::threads(0), |pool| {
            let view = PoolExecView::new(pool);
            let (a, b) = view.join(|| 10, || 20);
            assert_eq!(a + b, 30);
        });
    }

    #[test]
    fn fork_join_map_basic() {
        let items: Vec<i32> = (0..64).collect();
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let view = PoolExecView::new(pool);
            let results = fork_join_map(&view, &items, &|&x| x * 2, 0, 6);
            let expected: Vec<i32> = (0..64).map(|x| x * 2).collect();
            assert_eq!(results, expected);
        });
    }

    #[test]
    fn fork_join_map_preserves_order() {
        let items: Vec<usize> = (0..100).collect();
        WorkPool::with(WorkPoolSpec::threads(4), |pool| {
            let view = PoolExecView::new(pool);
            let results = fork_join_map(&view, &items, &|&x| x, 0, 8);
            assert_eq!(results, items);
        });
    }

    /// THE regression test.
    #[test]
    fn sequential_pool_stress() {
        for iteration in 0..20 {
            WorkPool::with(WorkPoolSpec::threads(3), |pool| {
                let view = PoolExecView::new(pool);
                let items: Vec<i32> = (0..64).collect();
                let results = fork_join_map(&view, &items, &|&x| x * 2, 0, 6);
                let expected: Vec<i32> = (0..64).map(|x| x * 2).collect();
                assert_eq!(results, expected, "iteration {iteration}");
            });
        }
    }

    #[test]
    fn concurrent_pools() {
        let t1 = std::thread::spawn(|| {
            WorkPool::with(WorkPoolSpec::threads(2), |pool| {
                let view = PoolExecView::new(pool);
                let (a, b) = view.join(|| 10, || 20);
                assert_eq!(a + b, 30);
            });
        });
        let t2 = std::thread::spawn(|| {
            WorkPool::with(WorkPoolSpec::threads(2), |pool| {
                let view = PoolExecView::new(pool);
                let (a, b) = view.join(|| 100, || 200);
                assert_eq!(a + b, 300);
            });
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }
}
