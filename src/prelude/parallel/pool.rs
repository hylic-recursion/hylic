//! WorkPool: fixed-size thread pool with scoped lifecycle.
//!
//! Uses crossbeam-deque's lock-free Injector for task distribution.
//! join() uses stack-allocated jobs — no heap allocation in the hot path.
//! Workers steal from the shared Injector (lock-free), sleeping on a
//! condvar only when no work is available.

use std::cell::UnsafeCell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use crossbeam_deque::{Injector, Steal};
use super::sync_unsafe::SyncRef;

// ── Task reference ───────────────────────────────────
//
// Type-erased handle to a unit of work. Two words: data pointer +
// monomorphized execute function. For stack-allocated jobs (join),
// `data` points into the caller's frame. For boxed closures (submit),
// `data` points to a heap-allocated wrapper.

struct TaskRef {
    data: *const (),
    execute: unsafe fn(*const ()),
}

// SAFETY: TaskRef is a raw function-pointer pair. The creator guarantees
// the pointed-to data outlives the task (join blocks, submit heap-allocs).
unsafe impl Send for TaskRef {}

impl TaskRef {
    #[inline]
    unsafe fn run(self) {
        unsafe { (self.execute)(self.data); }
    }
}

// ── Stack-allocated job ──────────────────────────────
//
// Rayon's StackJob pattern: the closure lives on the caller's stack.
// join() blocks until done, guaranteeing the frame outlives execution.
// No heap allocation — the Injector stores a two-word TaskRef that
// points back to this stack-local struct.

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

    /// Monomorphized execute function — called via function pointer but
    /// the body knows the concrete type F, so the closure call is direct.
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

// ── WorkPool ─────────────────────────────────────────

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
///
/// Task queue: crossbeam-deque Injector (lock-free MPMC).
/// Workers steal from the Injector on the fast path. The condvar is
/// only for sleep/wake when no work is available — never for queue access.
pub struct WorkPool {
    injector: Injector<TaskRef>,
    condvar: Condvar,
    wake_lock: Mutex<()>,
    shutdown: AtomicBool,
}

impl WorkPool {
    /// Create a pool, run `f` with access to it, shut down, join all workers.
    /// The pool cannot escape the closure. Workers are scoped threads —
    /// guaranteed joined on return, even on panic.
    pub fn with<R>(spec: WorkPoolSpec, f: impl FnOnce(&Arc<Self>) -> R) -> R {
        let pool = Arc::new(WorkPool {
            injector: Injector::new(),
            condvar: Condvar::new(),
            wake_lock: Mutex::new(()),
            shutdown: AtomicBool::new(false),
        });
        std::thread::scope(|s| {
            for _ in 0..spec.n_workers {
                s.spawn(|| pool.worker_loop());
            }
            struct ShutdownGuard<'a>(&'a WorkPool);
            impl Drop for ShutdownGuard<'_> {
                fn drop(&mut self) { self.0.shutdown(); }
            }
            let _guard = ShutdownGuard(&pool);
            f(&pool)
        })
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.condvar.notify_all();
    }

    fn worker_loop(&self) {
        loop {
            // Fast path: lock-free steal from injector
            if let Some(task) = self.try_steal() {
                unsafe { task.run(); }
                continue;
            }
            // Slow path: no work available, sleep on condvar.
            // The wake_lock is only for the sleep/wake protocol,
            // never for queue access.
            let guard = self.wake_lock.lock().unwrap();
            if self.shutdown.load(Ordering::Acquire) { return; }
            // Double-check after lock — work may have arrived between
            // the failed steal and the lock acquisition
            if let Some(task) = self.try_steal() {
                drop(guard);
                unsafe { task.run(); }
                continue;
            }
            let _guard = self.condvar.wait(guard).unwrap();
            if self.shutdown.load(Ordering::Acquire) { return; }
        }
    }

    fn try_steal(&self) -> Option<TaskRef> {
        loop {
            match self.injector.steal() {
                Steal::Success(task) => return Some(task),
                Steal::Empty => return None,
                Steal::Retry => continue,
            }
        }
    }

    fn wake_one(&self) {
        self.condvar.notify_one();
    }

    /// Submit a boxed closure to the pool.
    /// For fork-join, prefer join() — it avoids heap allocation.
    pub fn submit(&self, f: Box<dyn FnOnce() + Send>) {
        // Box the fat pointer into a thin pointer for TaskRef
        let wrapper = Box::into_raw(Box::new(f));
        let task = TaskRef {
            data: wrapper as *const (),
            execute: Self::execute_boxed,
        };
        self.injector.push(task);
        self.wake_one();
    }

    unsafe fn execute_boxed(ptr: *const ()) {
        unsafe {
            let wrapper = Box::from_raw(ptr as *mut Box<dyn FnOnce() + Send>);
            (*wrapper)();
        }
    }

    /// Returns true if the task queue is empty.
    pub fn is_idle(&self) -> bool {
        self.injector.is_empty()
    }

    /// Try to steal and execute one task. Returns true if work was found.
    pub fn try_run_one(&self) -> bool {
        if let Some(task) = self.try_steal() {
            unsafe { task.run(); }
            true
        } else {
            false
        }
    }

    /// Scoped fork-join: push f2 to the pool, run f1 on the current
    /// thread, then spin-help until f2 completes.
    ///
    /// Both closures may borrow from the caller's stack. The closure
    /// for f2 is stored on the stack (StackJob pattern) — no heap
    /// allocation. Safe because join() blocks until f2 completes.
    pub fn join<A: Send, B: Send>(
        &self,
        f1: impl FnOnce() -> A + Send,
        f2: impl FnOnce() -> B + Send,
    ) -> (A, B) {
        let job = StackJob::new(f2);

        // SAFETY: job lives on this stack frame. join() blocks until
        // the job completes, guaranteeing the frame outlives execution.
        // The TaskRef holds a raw pointer to job — valid for the
        // duration of this function.
        self.injector.push(job.as_task_ref());
        self.wake_one();

        // Execute f1, catching panics so we still wait for f2
        let result_a = catch_unwind(AssertUnwindSafe(f1));

        // Must wait for f2 regardless of f1's outcome — the StackJob
        // is on our stack and must not unwind while a worker executes it
        while !job.is_done() {
            if !self.try_run_one() {
                std::hint::spin_loop();
            }
        }

        // SAFETY: done flag guarantees result is written (Acquire/Release)
        let result_b = unsafe { (*job.result.get()).take().unwrap() };

        match (result_a, result_b) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => resume_unwind(e),
        }
    }
}

// ── fork_join_map ────────────────────────────────────

/// Binary-split fork-join over a slice. Recursively halves the work,
/// using pool.join() at each level. Sequential below max_depth or
/// when only one item remains.
///
/// Generic in F — the closure is monomorphized, not dispatched through
/// a vtable. No `T: Sync` bound — the slice is wrapped in SyncRef.
pub fn fork_join_map<T, R: Send, F: Fn(&T) -> R + Send + Sync>(
    pool: &WorkPool,
    items: &[T],
    f: &F,
    depth: usize,
    max_depth: usize,
) -> Vec<R> {
    let items = SyncRef(items);
    fork_join_map_inner(pool, &items, f, depth, max_depth)
}

fn fork_join_map_inner<T, R: Send, F: Fn(&T) -> R + Send + Sync>(
    pool: &WorkPool,
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
    let (left, right) = pool.join(
        || fork_join_map_inner(pool, &left_items, f, depth + 1, max_depth),
        || fork_join_map_inner(pool, &right_items, f, depth + 1, max_depth),
    );
    let mut result = left;
    result.extend(right);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_basic() {
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let (a, b) = pool.join(|| 1 + 2, || 3 + 4);
            assert_eq!((a, b), (3, 7));
        });
    }

    #[test]
    fn join_borrows_from_stack() {
        let data = vec![10, 20, 30];
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let (sum_left, sum_right) = pool.join(
                || data[..2].iter().sum::<i32>(),
                || data[2..].iter().sum::<i32>(),
            );
            assert_eq!(sum_left + sum_right, 60);
        });
    }

    #[test]
    fn join_nested() {
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let (a, b) = pool.join(
                || {
                    let (x, y) = pool.join(|| 1, || 2);
                    x + y
                },
                || {
                    let (x, y) = pool.join(|| 3, || 4);
                    x + y
                },
            );
            assert_eq!(a + b, 10);
        });
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn join_propagates_panic() {
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            pool.join(|| 1, || -> i32 { panic!("boom") });
        });
    }

    #[test]
    fn fork_join_map_basic() {
        let items: Vec<i32> = (0..8).collect();
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let results = fork_join_map(pool, &items, &|x| x * x, 0, 4);
            assert_eq!(results, vec![0, 1, 4, 9, 16, 25, 36, 49]);
        });
    }

    #[test]
    fn fork_join_map_sequential_fallback() {
        let items: Vec<i32> = (0..8).collect();
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            // max_depth=0 forces fully sequential
            let results = fork_join_map(pool, &items, &|x| x * x, 0, 0);
            assert_eq!(results, vec![0, 1, 4, 9, 16, 25, 36, 49]);
        });
    }

    #[test]
    fn fork_join_map_preserves_order() {
        let items: Vec<usize> = (0..100).collect();
        WorkPool::with(WorkPoolSpec::threads(4), |pool| {
            let results = fork_join_map(pool, &items, &|&x| x, 0, 8);
            assert_eq!(results, items);
        });
    }
}
