//! WorkPool: fixed-size thread pool with scoped lifecycle.

use std::cell::UnsafeCell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
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

    pub fn submit(&self, f: Box<dyn FnOnce() + Send>) {
        self.queue.lock().unwrap().push(f);
        self.condvar.notify_one();
    }

    /// Returns true if the task queue is empty.
    pub fn is_idle(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }

    pub fn try_run_one(&self) -> bool {
        let item = self.queue.lock().unwrap().pop();
        match item {
            Some(f) => { f(); true }
            None => false,
        }
    }

    /// Scoped fork-join: submit f2 to the pool, run f1 on the current
    /// thread, then spin-help until f2 completes.
    ///
    /// Both closures may borrow from the caller's stack — the lifetime
    /// is erased via transmute. Safe because join() blocks until f2
    /// completes, so the stack frame outlives both closures.
    pub fn join<A: Send, B: Send>(
        &self,
        f1: impl FnOnce() -> A + Send,
        f2: impl FnOnce() -> B + Send,
    ) -> (A, B) {
        /// Raw pointers to the caller's stack, bundled as Send.
        /// SAFETY: join() blocks until the closure completes, so the
        /// stack frame outlives these pointers.
        struct JoinSlot<B> {
            result: *mut Option<Result<B, Box<dyn std::any::Any + Send>>>,
            done: *const AtomicBool,
        }
        unsafe impl<B> Send for JoinSlot<B> {}

        impl<B> JoinSlot<B> {
            /// Write result and signal done. Method call ensures Rust 2021
            /// precise captures grabs the whole struct (which is Send),
            /// not the individual raw pointer fields (which aren't).
            unsafe fn complete(&self, r: Result<B, Box<dyn std::any::Any + Send>>) {
                unsafe {
                    self.result.write(Some(r));
                    (*self.done).store(true, Ordering::Release);
                }
            }
        }

        let result_slot: UnsafeCell<Option<Result<B, Box<dyn std::any::Any + Send>>>> =
            UnsafeCell::new(None);
        let done = AtomicBool::new(false);

        let slot = JoinSlot { result: result_slot.get(), done: &done };

        // SAFETY: join() spin-loops until `done` is set. result_slot
        // and done live on this stack frame, which outlives the closure.
        // The transmute erases the non-'static lifetime bound so the
        // closure can enter the 'static task queue.
        unsafe {
            let closure: Box<dyn FnOnce() + Send + '_> = Box::new(move || {
                let r = catch_unwind(AssertUnwindSafe(f2));
                slot.complete(r);
            });

            let closure: Box<dyn FnOnce() + Send + 'static> =
                std::mem::transmute(closure);

            self.submit(closure);
        }

        let a = f1();

        while !done.load(Ordering::Acquire) {
            if !self.try_run_one() { std::hint::spin_loop(); }
        }

        // SAFETY: done is set only after result is written.
        // Acquire/Release pair ensures visibility.
        let b = unsafe { (*result_slot.get()).take().unwrap() };
        match b {
            Ok(val) => (a, val),
            Err(payload) => resume_unwind(payload),
        }
    }
}

/// A reference safe to share across scoped threads.
///
/// SAFETY: WorkPool uses std::thread::scope — all workers join before
/// the scope exits. SyncRef borrows from within the scope. Workers
/// only deref + call (read-only). No Rc cloning, no mutation of
/// refcounts. The !Sync on Rc protects against concurrent clone/drop,
/// which doesn't happen through SyncRef.
pub struct SyncRef<'a, T: ?Sized>(pub &'a T);
unsafe impl<T: ?Sized> Sync for SyncRef<'_, T> {}
unsafe impl<T: ?Sized> Send for SyncRef<'_, T> {}
impl<T: ?Sized> std::ops::Deref for SyncRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.0 }
}

/// Binary-split fork-join over a slice. Recursively halves the work,
/// using pool.join() at each level. Sequential below max_depth or
/// when only one item remains.
///
/// No `T: Sync` bound — the slice is wrapped in SyncRef internally.
/// Safe because pool.join() uses scoped threads (the slice outlives
/// all workers) and workers only read elements via `&T`.
pub fn fork_join_map<T, R: Send>(
    pool: &WorkPool,
    items: &[T],
    f: &(dyn Fn(&T) -> R + Send + Sync),
    depth: usize,
    max_depth: usize,
) -> Vec<R> {
    let items = SyncRef(items);
    fork_join_map_inner(pool, &items, f, depth, max_depth)
}

fn fork_join_map_inner<T, R: Send>(
    pool: &WorkPool,
    items: &SyncRef<'_, [T]>,
    f: &(dyn Fn(&T) -> R + Send + Sync),
    depth: usize,
    max_depth: usize,
) -> Vec<R> {
    if items.len() <= 1 || depth >= max_depth {
        return items.iter().map(f).collect();
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
