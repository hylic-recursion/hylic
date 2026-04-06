//! Shared strategy: single StealQueue for all workers.
//!
//! All threads push to one queue. All threads steal from it.
//! No per-worker deques. No bitmask. Simple and contention-friendly
//! for small trees.

use crate::prelude::parallel::base::steal_queue::StealQueue;
use super::super::super::cont::FunnelTask;
use super::{WorkStealing, TaskOps};

pub struct Shared;

#[derive(Clone, Default)]
pub struct SharedSpec;

unsafe impl Send for SharedSpec {}
unsafe impl Sync for SharedSpec {}

pub struct SharedStore<N, H, R> {
    queue: StealQueue<FunnelTask<N, H, R>>,
}

unsafe impl<N: Send, H, R: Send> Send for SharedStore<N, H, R> {}
unsafe impl<N: Send, H, R: Send> Sync for SharedStore<N, H, R> {}

pub struct SharedHandle<'a, N, H, R> {
    queue: &'a StealQueue<FunnelTask<N, H, R>>,
}

impl WorkStealing for Shared {
    type Spec = SharedSpec;
    type Store<N: Send + 'static, H: 'static, R: Send + 'static> = SharedStore<N, H, R>;
    type Handle<'a, N: Send + 'static, H: 'static, R: Send + 'static> = SharedHandle<'a, N, H, R>;

    fn create_store<N: Send + 'static, H: 'static, R: Send + 'static>(
        _spec: &Self::Spec, _n_workers: usize,
    ) -> Self::Store<N, H, R> {
        SharedStore { queue: StealQueue::new() }
    }

    fn reset_store<N: Send + 'static, H: 'static, R: Send + 'static>(
        store: &mut Self::Store<N, H, R>,
    ) {
        // StealQueue is monotonic — can't reset indices. Create a new one.
        store.queue = StealQueue::new();
    }

    fn handle<'a, N: Send + 'static, H: 'static, R: Send + 'static>(
        store: &'a Self::Store<N, H, R>, _worker_idx: usize,
    ) -> Self::Handle<'a, N, H, R> {
        SharedHandle { queue: &store.queue }
    }
}

impl<N: Send + 'static, H: 'static, R: Send + 'static> TaskOps<N, H, R>
    for SharedHandle<'_, N, H, R>
{
    fn push(&self, task: FunnelTask<N, H, R>, notify: &dyn Fn()) {
        self.queue.push(task);
        notify();
    }

    fn pop(&self) -> Option<FunnelTask<N, H, R>> {
        self.queue.steal()
    }

    fn steal(&self) -> Option<FunnelTask<N, H, R>> {
        self.queue.steal()
    }
}
