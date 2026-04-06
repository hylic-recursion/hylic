//! PerWorker strategy: Chase-Lev deques + bitmask steal.
//!
//! Each worker owns a deque. Push is local (LIFO, no atomic).
//! Steal uses an AtomicU64 bitmask to find non-empty deques —
//! one atomic load instead of scanning N deques.

use std::sync::atomic::{AtomicU64, Ordering};

use super::super::super::cont::FunnelTask;
use super::super::super::deque::WorkerDeque;
use super::{WorkStealing, TaskOps};

pub struct PerWorker;

#[derive(Clone)]
pub struct PerWorkerSpec {
    pub deque_capacity: usize,
}

impl Default for PerWorkerSpec {
    fn default() -> Self { PerWorkerSpec { deque_capacity: 4096 } }
}

unsafe impl Send for PerWorkerSpec {}
unsafe impl Sync for PerWorkerSpec {}

pub struct PerWorkerStore<N, H, R> {
    deques: Vec<WorkerDeque<FunnelTask<N, H, R>>>,
    work_available: AtomicU64,
}

unsafe impl<N: Send, H, R: Send> Send for PerWorkerStore<N, H, R> {}
unsafe impl<N: Send, H, R: Send> Sync for PerWorkerStore<N, H, R> {}

pub struct PerWorkerHandle<'a, N, H, R> {
    my_deque: &'a WorkerDeque<FunnelTask<N, H, R>>,
    all_deques: &'a [WorkerDeque<FunnelTask<N, H, R>>],
    my_idx: usize,
    work_available: &'a AtomicU64,
}

impl WorkStealing for PerWorker {
    type Spec = PerWorkerSpec;
    type Store<N: Send + 'static, H: 'static, R: Send + 'static> = PerWorkerStore<N, H, R>;
    type Handle<'a, N: Send + 'static, H: 'static, R: Send + 'static> = PerWorkerHandle<'a, N, H, R>;

    fn create_store<N: Send + 'static, H: 'static, R: Send + 'static>(
        spec: &Self::Spec, n_workers: usize,
    ) -> Self::Store<N, H, R> {
        let cap = if spec.deque_capacity == 0 { 4096 } else { spec.deque_capacity };
        PerWorkerStore {
            deques: (0..n_workers + 1).map(|_| WorkerDeque::new(cap)).collect(),
            work_available: AtomicU64::new(0),
        }
    }

    fn reset_store<N: Send + 'static, H: 'static, R: Send + 'static>(
        store: &mut Self::Store<N, H, R>,
    ) {
        *store.work_available.get_mut() = 0;
    }

    fn handle<'a, N: Send + 'static, H: 'static, R: Send + 'static>(
        store: &'a Self::Store<N, H, R>, worker_idx: usize,
    ) -> Self::Handle<'a, N, H, R> {
        PerWorkerHandle {
            my_deque: &store.deques[worker_idx],
            all_deques: &store.deques,
            my_idx: worker_idx,
            work_available: &store.work_available,
        }
    }
}

impl<N: Send + 'static, H: 'static, R: Send + 'static> TaskOps<N, H, R>
    for PerWorkerHandle<'_, N, H, R>
{
    fn push(&self, task: FunnelTask<N, H, R>, notify: &dyn Fn()) {
        assert!(self.my_deque.push(task), "deque full");
        self.work_available.fetch_or(1u64 << self.my_idx, Ordering::Relaxed);
        notify();
    }

    fn pop(&self) -> Option<FunnelTask<N, H, R>> {
        self.my_deque.pop()
    }

    fn steal(&self) -> Option<FunnelTask<N, H, R>> {
        let mut bits = self.work_available.load(Ordering::Relaxed);
        bits &= !(1u64 << self.my_idx);
        while bits != 0 {
            let target = bits.trailing_zeros() as usize;
            if let Some(task) = self.all_deques[target].steal() {
                return Some(task);
            }
            self.work_available.fetch_and(!(1u64 << target), Ordering::Relaxed);
            bits &= !(1u64 << target);
        }
        None
    }
}
