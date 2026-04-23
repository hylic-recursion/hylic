//! Funnel: bare-metal CPS parallel hylomorphism executor.
//!
//! Generic over P: FunnelPolicy, which bundles three behavioral axes:
//! - Queue topology (PerWorker, Shared)
//! - Accumulation strategy (OnArrival, OnFinalize)
//! - Wake policy (EveryPush, OncePerBatch, EveryK)
//!
//! All axes are monomorphized. Zero runtime overhead for strategy dispatch.

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

pub(crate) mod cps;
pub(crate) mod dispatch;
pub(crate) mod infra;
pub mod policy;
pub mod pool;

pub use policy::{queue, accumulate, wake};
pub use pool::Pool;
pub use infra::steal_queue::StealQueue;

use crate::domain::Domain;
use super::super::{Executor, ExecutorSpec};
use policy::FunnelPolicy;
use accumulate::AccumulateStrategy;
use queue::WorkStealing;
use wake::WakeStrategy;

// ANCHOR: funnel_spec
pub struct Spec<P: FunnelPolicy = policy::Default> {
    /// Pool size for `.run()` and `.session()`. Not consulted when
    /// attaching to an explicit pool via `.attach()`.
    pub default_pool_size: usize,
    pub queue: <P::Queue as WorkStealing>::Spec,
    pub accumulate: <P::Accumulate as AccumulateStrategy>::Spec,
    pub wake: <P::Wake as WakeStrategy>::Spec,
}
// ANCHOR_END: funnel_spec

impl<P: FunnelPolicy> Clone for Spec<P> {
    fn clone(&self) -> Self {
        Spec {
            default_pool_size: self.default_pool_size,
            queue: self.queue,
            accumulate: self.accumulate,
            wake: self.wake,
        }
    }
}
impl<P: FunnelPolicy> Copy for Spec<P> {}

impl<P: FunnelPolicy> std::fmt::Debug for Spec<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spec")
            .field("default_pool_size", &self.default_pool_size)
            .finish()
    }
}

impl<P: FunnelPolicy> std::fmt::Debug for Session<'_, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("n_workers", &self.pool_state.n_threads)
            .finish()
    }
}

// ── Default constructor (THE one source of defaults) ──

impl Spec<policy::Default> {
    pub fn default(n_workers: usize) -> Self {
        Spec { default_pool_size: n_workers,
            queue: queue::per_worker::PerWorkerSpec { deque_capacity: 4096 },
            accumulate: accumulate::on_finalize::OnFinalizeSpec,
            wake: wake::every_push::EveryPushSpec,
        }
    }
}

// ── Named presets (transformations of default) ────

impl Spec<policy::GraphHeavy> {
    pub fn for_graph_heavy(n_workers: usize) -> Self {
        Spec::default(n_workers)
    }
}

impl Spec<policy::PerWorkerArrival> {
    pub fn for_perworker_arrival(n_workers: usize) -> Self {
        Spec::default(n_workers)
            .with_accumulate::<accumulate::OnArrival>(accumulate::on_arrival::OnArrivalSpec)
    }
}

impl Spec<policy::SharedDefault> {
    pub fn for_shared_default(n_workers: usize) -> Self {
        Spec::default(n_workers)
            .with_queue::<queue::Shared>(queue::shared::SharedSpec)
    }
}

impl Spec<policy::WideLight> {
    pub fn for_wide_light(n_workers: usize) -> Self {
        Spec::default(n_workers)
            .with_queue::<queue::Shared>(queue::shared::SharedSpec)
            .with_accumulate::<accumulate::OnArrival>(accumulate::on_arrival::OnArrivalSpec)
    }
}

impl Spec<policy::LowOverhead> {
    pub fn for_low_overhead(n_workers: usize) -> Self {
        Spec::default(n_workers)
            .with_wake::<wake::OncePerBatch>(wake::once_per_batch::OncePerBatchSpec)
    }
}

impl Spec<policy::HighThroughput> {
    pub fn for_high_throughput(n_workers: usize) -> Self {
        Spec::default(n_workers)
            .with_wake::<wake::EveryK<4>>(wake::every_k::EveryKSpec)
    }
}

impl Spec<policy::StreamingWide> {
    pub fn for_streaming_wide(n_workers: usize) -> Self {
        Spec::default(n_workers)
            .with_queue::<queue::Shared>(queue::shared::SharedSpec)
            .with_accumulate::<accumulate::OnArrival>(accumulate::on_arrival::OnArrivalSpec)
            .with_wake::<wake::OncePerBatch>(wake::once_per_batch::OncePerBatchSpec)
    }
}

impl Spec<policy::DeepNarrow> {
    pub fn for_deep_narrow(n_workers: usize) -> Self {
        Spec::default(n_workers)
            .with_queue::<queue::PerWorker>(queue::per_worker::PerWorkerSpec { deque_capacity: 2048 })
            .with_wake::<wake::EveryK<2>>(wake::every_k::EveryKSpec)
    }
}

impl<P: FunnelPolicy> Spec<P> {
    pub fn new(
        n_workers: usize,
        queue: <P::Queue as WorkStealing>::Spec,
        accumulate: <P::Accumulate as AccumulateStrategy>::Spec,
        wake: <P::Wake as WakeStrategy>::Spec,
    ) -> Self {
        Spec { default_pool_size: n_workers, queue, accumulate, wake }
    }
}

// ── Spec: lifecycle ─────────────────────────────────

impl<P: FunnelPolicy> ExecutorSpec for Spec<P> {
    type Resource<'r> = &'r Pool<'r>;
    type Session<'s> = Session<'s, P>;

    fn attach(self, pool: <Self as ExecutorSpec>::Resource<'_>) -> Session<'_, P> {
        Session { pool_state: pool.state, spec: self }
    }

    fn with_session<R>(&self, f: impl for<'s> FnOnce(&Session<'s, P>) -> R) -> R {
        Pool::with(self.default_pool_size, |pool| f(&(*self).attach(pool)))
    }
}

// ── Spec: computation (routes through with_session) ─

impl<N, R, D: Domain<N>, G, P: FunnelPolicy> Executor<N, R, D, G> for Spec<P>
where
    N: Clone + Send + 'static, R: Send + 'static,
    G: crate::ops::TreeOps<N> + Send + Sync + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &G, root: &N) -> R {
        self.with_session(|session| Executor::<N, R, D, G>::run(session, fold, graph, root))
    }
}

// ── Session ─────────────────────────────────────────

// ANCHOR: funnel_session
pub struct Session<'s, P: FunnelPolicy = policy::Default> {
    pool_state: &'s pool::PoolState,
    spec: Spec<P>,
}
// ANCHOR_END: funnel_session

// ── Session: computation (direct dispatch) ──────────

impl<N, R, D: Domain<N>, G, P: FunnelPolicy> Executor<N, R, D, G> for Session<'_, P>
where
    N: Clone + Send + 'static, R: Send + 'static,
    G: crate::ops::TreeOps<N> + Send + Sync + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &G, root: &N) -> R {
        dispatch::run_fold::<_, _, _, _, _, P>(fold, graph, root, self.pool_state, &self.spec)
    }
}

#[cfg(test)]
mod tests;
