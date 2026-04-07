//! Work-stealing queue abstraction.
//!
//! Each strategy implements `WorkStealing` with associated types for
//! per-fold resources (Store) and per-worker handles (Handle).
//! The rest of the system interacts through `TaskOps` — push, try_acquire.

pub mod per_worker;
pub mod shared;

pub use per_worker::PerWorker;
pub use shared::Shared;

use super::super::cont::FunnelTask;

/// Per-worker task operations. Each WorkStealing::Handle implements this.
///
/// `push` submits a task. Returns None on success, Some(task) if the
/// queue is full (caller should execute inline). Notification of idle
/// workers is the caller's responsibility (WorkerCtx holds the view).
///
/// `try_acquire` returns the next task to execute. Each strategy
/// encapsulates its own acquisition policy:
/// - PerWorker: pop local deque first, then bitmask-guided steal
/// - Shared: steal from the global queue
pub trait TaskOps<N, H, R> {
    fn push(&self, task: FunnelTask<N, H, R>) -> Option<FunnelTask<N, H, R>>;
    fn try_acquire(&self) -> Option<FunnelTask<N, H, R>>;
}

/// A work-stealing strategy. Associates typed Store and Handle via GATs.
pub trait WorkStealing: 'static {
    type Spec: Clone + Default + Send + Sync;

    type Store<N: Send + 'static, H: 'static, R: Send + 'static>: Send + Sync;

    type Handle<'a, N: Send + 'static, H: 'static, R: Send + 'static>: TaskOps<N, H, R>
    where Self: 'a;

    fn create_store<N: Send + 'static, H: 'static, R: Send + 'static>(
        spec: &Self::Spec, n_workers: usize,
    ) -> Self::Store<N, H, R>;

    fn reset_store<N: Send + 'static, H: 'static, R: Send + 'static>(
        store: &mut Self::Store<N, H, R>,
    );

    fn handle<'a, N: Send + 'static, H: 'static, R: Send + 'static>(
        store: &'a Self::Store<N, H, R>, worker_idx: usize,
    ) -> Self::Handle<'a, N, H, R>;
}
