//! WorkPool + PoolExecView: scoped thread pool with zero globals.
//!
//! WorkPool owns worker threads and a view registry.
//! PoolExecView is the scoped execution coordinator — owns a SharedDeque
//! (via Arc), provides join(), fork_join_map(), help_once().
//!
//! ViewHandle is a cloneable handle to a view's deque + wake signal.
//! No raw pointers for deque or pool access — Arc provides stable
//! heap addresses and sound lifetime management.
//!
//! No thread_local, no process globals.

use std::cell::UnsafeCell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::deque::SharedDeque;
use super::sync_unsafe::SyncRef;

// ── Task reference ───────────────────────────────────

pub(crate) struct TaskRef {
    data: *const (),
    execute: unsafe fn(*const ()),
}

unsafe impl Send for TaskRef {}

impl TaskRef {
    #[inline]
    unsafe fn run(self) {
        unsafe { (self.execute)(self.data); }
    }

    pub(crate) fn from_boxed(f: Box<dyn FnOnce() + Send>) -> Self {
        let wrapper = Box::into_raw(Box::new(f));
        TaskRef {
            data: wrapper as *const (),
            execute: execute_boxed,
        }
    }
}

unsafe fn execute_boxed(ptr: *const ()) {
    unsafe {
        let wrapper = Box::from_raw(ptr as *mut Box<dyn FnOnce() + Send>);
        (*wrapper)();
    }
}

// ── Stack-allocated job ──────────────────────────────

struct StackJob<F, R> {
    func: UnsafeCell<Option<F>>,
    result: UnsafeCell<Option<Result<R, Box<dyn std::any::Any + Send>>>>,
    done: AtomicBool,
}

impl<F: FnOnce() -> R + Send, R: Send> StackJob<F, R> {
    fn new(f: F) -> Self {
        StackJob {
            func: UnsafeCell::new(Some(f)),
            result: UnsafeCell::new(None),
            done: AtomicBool::new(false),
        }
    }

    fn as_task_ref(&self) -> TaskRef {
        TaskRef {
            data: self as *const _ as *const (),
            execute: Self::execute_fn,
        }
    }

    unsafe fn execute_fn(ptr: *const ()) {
        unsafe {
            let this = &*(ptr as *const Self);
            let f = (*this.func.get()).take().unwrap();
            let r = catch_unwind(AssertUnwindSafe(f));
            *this.result.get() = Some(r);
            this.done.store(true, Ordering::Release);
        }
    }

    #[inline]
    fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }
}

// ── WakeSignal ───────────────────────────────────────

/// Worker sleep/wake mechanism. Shared via Arc between pool and views.
struct WakeSignal {
    condvar: Condvar,
    lock: Mutex<()>,
}

impl WakeSignal {
    fn new() -> Self {
        WakeSignal { condvar: Condvar::new(), lock: Mutex::new(()) }
    }

    fn wake_one(&self) {
        self.condvar.notify_one();
    }

    fn wake_all(&self) {
        self.condvar.notify_all();
    }
}

// ── ViewHandle ───────────────────────────────────────

/// Cloneable handle to a view's deque + wake signal.
/// No raw pointers — Arc provides stable heap addresses.
/// Send + Sync via Arc.
#[derive(Clone)]
pub struct ViewHandle {
    deque: Arc<SharedDeque<TaskRef>>,
    signal: Arc<WakeSignal>,
    views: Arc<Mutex<Vec<Arc<SharedDeque<TaskRef>>>>>,
}

impl ViewHandle {
    /// Push a boxed closure as a task.
    pub fn submit(&self, f: Box<dyn FnOnce() + Send>) {
        self.deque.push(TaskRef::from_boxed(f));
        self.signal.wake_one();
    }

    /// Try to find and run one task. LIFO pop from own deque first,
    /// then steal from any registered view.
    pub fn help_once(&self) -> bool {
        if let Some(task) = self.deque.pop() {
            unsafe { task.run(); }
            return true;
        }
        if let Some(task) = steal_from_views(&self.views) {
            unsafe { task.run(); }
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
    pub fn threads(n: usize) -> Self {
        WorkPoolSpec { n_workers: n }
    }
}

/// Fixed-size thread pool. Owns worker threads, a wake signal, and
/// a view registry. Zero work methods — all work goes through views.
pub struct WorkPool {
    views: Arc<Mutex<Vec<Arc<SharedDeque<TaskRef>>>>>,
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
    views: &Arc<Mutex<Vec<Arc<SharedDeque<TaskRef>>>>>,
    signal: &Arc<WakeSignal>,
    shutdown: &AtomicBool,
) {
    loop {
        if let Some(task) = steal_from_views(views) {
            unsafe { task.run(); }
            continue;
        }
        let guard = signal.lock.lock().unwrap();
        if shutdown.load(Ordering::Acquire) { return; }
        if let Some(task) = steal_from_views(views) {
            drop(guard);
            unsafe { task.run(); }
            continue;
        }
        let _guard = signal.condvar.wait(guard).unwrap();
        if shutdown.load(Ordering::Acquire) { return; }
    }
}

/// Steal one task from any registered view's deque (FIFO).
fn steal_from_views(
    views: &Mutex<Vec<Arc<SharedDeque<TaskRef>>>>,
) -> Option<TaskRef> {
    let views = views.lock().unwrap();
    for deque in views.iter() {
        if let Some(task) = deque.steal() {
            return Some(task);
        }
    }
    None
}

// ── PoolExecView ─────────────────────────────────────

/// Scoped execution coordinator. Owns a SharedDeque (via Arc).
/// Registered with the pool on creation, deregistered on drop.
pub struct PoolExecView {
    deque: Arc<SharedDeque<TaskRef>>,
    signal: Arc<WakeSignal>,
    views: Arc<Mutex<Vec<Arc<SharedDeque<TaskRef>>>>>,
}

impl PoolExecView {
    pub fn new(pool: &WorkPool) -> Self {
        let deque = Arc::new(SharedDeque::new());
        pool.views.lock().unwrap().push(deque.clone());
        PoolExecView {
            deque,
            signal: pool.signal.clone(),
            views: pool.views.clone(),
        }
    }

    /// Create a ViewHandle for closures that need deque access.
    pub fn handle(&self) -> ViewHandle {
        ViewHandle {
            deque: self.deque.clone(),
            signal: self.signal.clone(),
            views: self.views.clone(),
        }
    }

    /// Scoped fork-join. Push f2, run f1, spin-help until done.
    pub fn join<A: Send, B: Send>(
        &self,
        f1: impl FnOnce() -> A + Send,
        f2: impl FnOnce() -> B + Send,
    ) -> (A, B) {
        let job = StackJob::new(f2);
        self.deque.push(job.as_task_ref());
        self.signal.wake_one();

        let result_a = catch_unwind(AssertUnwindSafe(f1));

        while !job.is_done() {
            if !self.help_once() {
                std::hint::spin_loop();
            }
        }

        let result_b = unsafe { (*job.result.get()).take().unwrap() };

        match (result_a, result_b) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => resume_unwind(e),
        }
    }

    /// Try to find and run one task. LIFO pop first, then steal.
    pub fn help_once(&self) -> bool {
        if let Some(task) = self.deque.pop() {
            unsafe { task.run(); }
            return true;
        }
        if let Some(task) = steal_from_views(&self.views) {
            unsafe { task.run(); }
            return true;
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.deque.is_empty()
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
    let left_items = SyncRef(&items[..mid]);
    let right_items = SyncRef(&items[mid..]);
    let (left, right) = view.join(
        || fork_join_map_inner(view, &left_items, f, depth + 1, max_depth),
        || fork_join_map_inner(view, &right_items, f, depth + 1, max_depth),
    );
    let mut result = left;
    result.extend(right);
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

    /// Reproducer: exercises the full sequence that the test suite runs —
    /// pool creation, fork_join_map (workers stealing from the view),
    /// pool teardown, then a second pool doing the same.
    /// If the deque has a corruption bug, the second pool's deque
    /// will see garbage (e.g., 8TB allocation in grow).
    #[test]
    fn sequential_pool_fork_join_map_stress() {
        for iteration in 0..20 {
            eprintln!("[stress] iteration {iteration}");
            WorkPool::with(WorkPoolSpec::threads(3), |pool| {
                let view = PoolExecView::new(pool);
                let items: Vec<i32> = (0..64).collect();
                let results = fork_join_map(&view, &items, &|&x| x * 2, 0, 6);
                let expected: Vec<i32> = (0..64).map(|x| x * 2).collect();
                assert_eq!(results, expected, "iteration {iteration}");
            });
            eprintln!("[stress] iteration {iteration} done");
        }
    }

    #[test]
    fn join_borrows_from_stack() {
        let data = vec![10, 20, 30];
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let view = PoolExecView::new(pool);
            let (sum_left, sum_right) = view.join(
                || data[..2].iter().sum::<i32>(),
                || data[2..].iter().sum::<i32>(),
            );
            assert_eq!(sum_left + sum_right, 60);
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
    fn fork_join_map_basic() {
        let items: Vec<i32> = (0..8).collect();
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let view = PoolExecView::new(pool);
            let results = fork_join_map(&view, &items, &|x| x * x, 0, 4);
            assert_eq!(results, vec![0, 1, 4, 9, 16, 25, 36, 49]);
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

    #[test]
    fn join_zero_workers() {
        WorkPool::with(WorkPoolSpec::threads(0), |pool| {
            let view = PoolExecView::new(pool);
            let (a, b) = view.join(|| 1 + 2, || 3 + 4);
            assert_eq!((a, b), (3, 7));
        });
    }

    #[test]
    fn sequential_pool_reuse() {
        for _ in 0..5 {
            WorkPool::with(WorkPoolSpec::threads(3), |pool| {
                let view = PoolExecView::new(pool);
                let (a, b) = view.join(
                    || view.join(|| 1, || 2),
                    || view.join(|| 3, || 4),
                );
                assert_eq!(a.0 + a.1 + b.0 + b.1, 10);
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

    #[test]
    fn view_handle_submit_and_help() {
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let view = PoolExecView::new(pool);
            let vh = view.handle();

            use std::sync::atomic::AtomicI32;
            let result = Arc::new(AtomicI32::new(0));
            let r = result.clone();
            vh.submit(Box::new(move || {
                r.store(42, Ordering::Release);
            }));
            // Spin until the task completes (via workers or help_once)
            while result.load(Ordering::Acquire) != 42 {
                vh.help_once();
            }
            assert_eq!(result.load(Ordering::Acquire), 42);
        });
    }
}
