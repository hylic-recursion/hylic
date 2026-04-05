//! FunnelPool: per-worker deques with work-stealing.
//!
//! Each worker owns a typed WorkerDeque. Tasks are data (FunnelTask enum),
//! not closures. Push is local (no atomic). Steal is a rare CAS.
//! EventCount for parking. No Mutex, no shared queue.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::deque::WorkerDeque;
use super::eventcount::EventCount;

const DEQUE_CAPACITY: usize = 4096;

pub(super) struct FunnelPoolSpec {
    pub n_workers: usize,
}

impl FunnelPoolSpec {
    pub fn threads(n: usize) -> Self { FunnelPoolSpec { n_workers: n } }
}

/// Shared state visible to all workers + the calling thread.
pub(super) struct FunnelPoolShared {
    pub event: EventCount,
    pub shutdown: AtomicBool,
    pub idle_count: AtomicU32,
    pub n_workers: usize,
}

impl FunnelPoolShared {
    pub fn notify_one(&self) {
        if self.idle_count.load(Ordering::Relaxed) > 0 {
            self.event.notify_one();
        }
    }
}

/// Runs the scoped thread pool. Workers are typed over the task type T.
/// The caller provides a body that receives the shared state and an array
/// of deque references. Deque index `n_workers` is the calling thread's.
pub(super) fn with_pool<T: Send, R>(
    spec: FunnelPoolSpec,
    body: impl FnOnce(&FunnelPoolShared, &[WorkerDeque<T>]) -> R,
    worker_fn: impl Fn(&FunnelPoolShared, &[WorkerDeque<T>], usize) + Send + Sync,
) -> R {
    let n = spec.n_workers;
    let deques: Vec<WorkerDeque<T>> = (0..n + 1).map(|_| WorkerDeque::new(DEQUE_CAPACITY)).collect();
    let shared = FunnelPoolShared {
        event: EventCount::new(),
        shutdown: AtomicBool::new(false),
        idle_count: AtomicU32::new(0),
        n_workers: n,
    };

    std::thread::scope(|s| {
        let wf = &worker_fn;
        for i in 0..n {
            let shared_ref = &shared;
            let deques_ref = deques.as_slice();
            s.spawn(move || wf(shared_ref, deques_ref, i));
        }
        struct ShutdownGuard<'a>(&'a FunnelPoolShared);
        impl Drop for ShutdownGuard<'_> {
            fn drop(&mut self) {
                self.0.shutdown.store(true, Ordering::Release);
                self.0.event.notify_all();
            }
        }
        let _guard = ShutdownGuard(&shared);
        body(&shared, &deques)
    })
}

/// Try to steal a task from any deque other than `my_idx`.
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// Simple typed task for pool-level tests. No walk_cps dependency.
    enum TestTask {
        Increment(Arc<AtomicU64>),
    }

    unsafe impl Send for TestTask {}

    fn test_worker(
        shared: &FunnelPoolShared,
        deques: &[WorkerDeque<TestTask>],
        my_idx: usize,
    ) {
        let my_deque = &deques[my_idx];
        loop {
            if let Some(TestTask::Increment(c)) = my_deque.pop() {
                c.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if let Some(TestTask::Increment(c)) = steal_from_others(deques, my_idx) {
                c.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let token = shared.event.prepare();
            if shared.shutdown.load(Ordering::Acquire) { return; }
            if let Some(TestTask::Increment(c)) = my_deque.pop() {
                c.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if let Some(TestTask::Increment(c)) = steal_from_others(deques, my_idx) {
                c.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            shared.idle_count.fetch_add(1, Ordering::Relaxed);
            shared.event.wait(token);
            shared.idle_count.fetch_sub(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn basic_submit_and_steal() {
        let counter = Arc::new(AtomicU64::new(0));
        with_pool(
            FunnelPoolSpec::threads(2),
            |shared, deques| {
                let caller_deque = &deques[shared.n_workers];
                for _ in 0..10 {
                    caller_deque.push(TestTask::Increment(counter.clone()));
                    shared.notify_one();
                }
                while counter.load(Ordering::Relaxed) < 10 {
                    if let Some(TestTask::Increment(c)) = caller_deque.pop() {
                        c.fetch_add(1, Ordering::Relaxed);
                    } else {
                        std::hint::spin_loop();
                    }
                }
                assert_eq!(counter.load(Ordering::Relaxed), 10);
            },
            test_worker,
        );
    }

    #[test]
    fn zero_workers_caller_processes_all() {
        let counter = Arc::new(AtomicU64::new(0));
        with_pool(
            FunnelPoolSpec::threads(0),
            |shared, deques| {
                let caller_deque = &deques[shared.n_workers];
                for _ in 0..20 {
                    caller_deque.push(TestTask::Increment(counter.clone()));
                }
                while counter.load(Ordering::Relaxed) < 20 {
                    if let Some(TestTask::Increment(c)) = caller_deque.pop() {
                        c.fetch_add(1, Ordering::Relaxed);
                    }
                }
                assert_eq!(counter.load(Ordering::Relaxed), 20);
            },
            test_worker,
        );
    }

    #[test]
    fn lifecycle_stress_500() {
        for _ in 0..500 {
            with_pool(
                FunnelPoolSpec::threads(4),
                |_, _| {},
                test_worker,
            );
        }
    }

    #[test]
    fn lifecycle_with_work_500() {
        for _ in 0..500 {
            let counter = Arc::new(AtomicU64::new(0));
            with_pool(
                FunnelPoolSpec::threads(4),
                |shared, deques| {
                    let caller_deque = &deques[shared.n_workers];
                    caller_deque.push(TestTask::Increment(counter.clone()));
                    shared.notify_one();
                    while counter.load(Ordering::Relaxed) < 1 {
                        if let Some(TestTask::Increment(c)) = caller_deque.pop() {
                            c.fetch_add(1, Ordering::Relaxed);
                        } else {
                            std::hint::spin_loop();
                        }
                    }
                },
                test_worker,
            );
        }
    }

    #[test]
    fn workers_steal_from_caller() {
        let counter = Arc::new(AtomicU64::new(0));
        with_pool(
            FunnelPoolSpec::threads(3),
            |shared, deques| {
                let caller_deque = &deques[shared.n_workers];
                // Push 100 tasks to caller's deque. Workers steal them.
                for _ in 0..100 {
                    caller_deque.push(TestTask::Increment(counter.clone()));
                }
                shared.event.notify_all();
                while counter.load(Ordering::Relaxed) < 100 {
                    if let Some(TestTask::Increment(c)) = caller_deque.pop() {
                        c.fetch_add(1, Ordering::Relaxed);
                    } else {
                        std::hint::spin_loop();
                    }
                }
            },
            test_worker,
        );
    }
}
