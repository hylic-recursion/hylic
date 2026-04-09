//! Funnel: bare-metal CPS parallel hylomorphism executor.
//!
//! Generic over P: FunnelPolicy, which bundles three behavioral axes:
//! - Queue topology (PerWorker, Shared)
//! - Accumulation strategy (OnArrival, OnFinalize)
//! - Wake policy (EveryPush, OncePerBatch, EveryK)
//!
//! All axes are monomorphized. Zero runtime overhead for strategy dispatch.

pub(crate) mod cps;
pub mod dispatch;
pub(crate) mod infra;
pub mod policy;
pub mod pool;

pub use policy::{queue, accumulate, wake};
pub use pool::Pool;

use crate::domain::Domain;
use super::super::{Executor, ExecutorSpec};
use policy::FunnelPolicy;
use accumulate::AccumulateStrategy;
use queue::WorkStealing;
use wake::WakeStrategy;

// ANCHOR: funnel_spec
pub struct Spec<P: FunnelPolicy = policy::Default> {
    pub n_workers: usize,
    pub chain_arena_capacity: usize,
    pub cont_arena_capacity: usize,
    pub queue: <P::Queue as WorkStealing>::Spec,
    pub accumulate: <P::Accumulate as AccumulateStrategy>::Spec,
    pub wake: <P::Wake as WakeStrategy>::Spec,
}
// ANCHOR_END: funnel_spec

impl Spec<policy::Default> {
    pub fn default(n_workers: usize) -> Self {
        Spec { n_workers, chain_arena_capacity: 4096, cont_arena_capacity: 8192,
            queue: queue::per_worker::PerWorkerSpec { deque_capacity: 4096 },
            accumulate: accumulate::on_finalize::OnFinalizeSpec,
            wake: wake::every_push::EveryPushSpec,
        }
    }
}

impl Spec<policy::GraphHeavy> {
    pub fn for_graph_heavy(n_workers: usize) -> Self {
        Spec { n_workers, chain_arena_capacity: 8192, cont_arena_capacity: 16384,
            queue: queue::per_worker::PerWorkerSpec { deque_capacity: 4096 },
            accumulate: accumulate::on_finalize::OnFinalizeSpec,
            wake: wake::every_push::EveryPushSpec,
        }
    }
}

impl Spec<policy::WideLight> {
    pub fn for_wide_light(n_workers: usize) -> Self {
        Spec { n_workers, chain_arena_capacity: 4096, cont_arena_capacity: 8192,
            queue: queue::shared::SharedSpec,
            accumulate: accumulate::on_arrival::OnArrivalSpec,
            wake: wake::every_push::EveryPushSpec,
        }
    }
}

impl Spec<policy::LowOverhead> {
    pub fn for_low_overhead(n_workers: usize) -> Self {
        Spec { n_workers, chain_arena_capacity: 4096, cont_arena_capacity: 8192,
            queue: queue::per_worker::PerWorkerSpec { deque_capacity: 4096 },
            accumulate: accumulate::on_finalize::OnFinalizeSpec,
            wake: wake::once_per_batch::OncePerBatchSpec,
        }
    }
}

impl Spec<policy::HighThroughput> {
    pub fn for_high_throughput(n_workers: usize) -> Self {
        Spec { n_workers, chain_arena_capacity: 4096, cont_arena_capacity: 8192,
            queue: queue::per_worker::PerWorkerSpec { deque_capacity: 4096 },
            accumulate: accumulate::on_finalize::OnFinalizeSpec,
            wake: wake::every_k::EveryKSpec,
        }
    }
}

impl Spec<policy::StreamingWide> {
    pub fn for_streaming_wide(n_workers: usize) -> Self {
        Spec { n_workers, chain_arena_capacity: 4096, cont_arena_capacity: 8192,
            queue: queue::shared::SharedSpec,
            accumulate: accumulate::on_arrival::OnArrivalSpec,
            wake: wake::once_per_batch::OncePerBatchSpec,
        }
    }
}

impl Spec<policy::DeepNarrow> {
    pub fn for_deep_narrow(n_workers: usize) -> Self {
        Spec { n_workers, chain_arena_capacity: 2048, cont_arena_capacity: 4096,
            queue: queue::per_worker::PerWorkerSpec { deque_capacity: 2048 },
            accumulate: accumulate::on_finalize::OnFinalizeSpec,
            wake: wake::every_k::EveryKSpec,
        }
    }
}

impl<P: FunnelPolicy> Spec<P> {
    pub fn new(
        n_workers: usize,
        queue: <P::Queue as WorkStealing>::Spec,
        accumulate: <P::Accumulate as AccumulateStrategy>::Spec,
        wake: <P::Wake as WakeStrategy>::Spec,
    ) -> Self {
        Spec { n_workers, chain_arena_capacity: 4096, cont_arena_capacity: 8192, queue, accumulate, wake }
    }

    pub fn with_arena_capacity(mut self, chains: usize, conts: usize) -> Self {
        self.chain_arena_capacity = chains;
        self.cont_arena_capacity = conts;
        self
    }

    /// Bind a pre-created pool to this spec, producing a Session.
    /// The session borrows both the pool (threads) and the spec (config).
    /// Per-fold memory (arenas, stores) is allocated fresh each run.
    pub fn bind<'a>(&'a self, pool: &'a Pool<'_>) -> Session<'a, P> {
        Session { pool_state: pool.state, spec: self }
    }
}

// ── Spec: lifecycle (creates scoped pool) ───────────

impl<P: FunnelPolicy> ExecutorSpec for Spec<P> {
    type Session<'s> = Session<'s, P>;
    fn with_session<R>(&self, f: impl for<'s> FnOnce(&Session<'s, P>) -> R) -> R {
        Pool::with(self.n_workers, |pool| f(&self.bind(pool)))
    }
}

// ── Spec: computation (routes through with_session) ─

impl<N, R, D: Domain<N>, P: FunnelPolicy> Executor<N, R, D> for Spec<P>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        self.with_session(|session| Executor::<N, R, D>::run(session, fold, graph, root))
    }
}

// ── Session ─────────────────────────────────────────

// ANCHOR: funnel_session
pub struct Session<'s, P: FunnelPolicy = policy::Default> {
    pool_state: &'s pool::PoolState,
    spec: &'s Spec<P>,
}
// ANCHOR_END: funnel_session

// ── Session: lifecycle (identity) ───────────────────

impl<P: FunnelPolicy> ExecutorSpec for Session<'_, P> {
    type Session<'s> = Self where Self: 's;
    fn with_session<R>(&self, f: impl for<'s> FnOnce(&Self) -> R) -> R { f(self) }
}

// ── Session: computation (direct dispatch) ──────────

impl<N, R, D: Domain<N>, P: FunnelPolicy> Executor<N, R, D> for Session<'_, P>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        dispatch::run_fold::<_, _, _, _, _, P>(fold, graph, root, self.pool_state, self.spec)
    }
}

#[cfg(test)]
mod tests;
