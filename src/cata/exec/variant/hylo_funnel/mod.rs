//! Hylo-funnel: bare-metal CPS parallel hylomorphism executor.
//!
//! Generic over work-stealing strategy (W: WorkStealing).
//! Default: PerWorker (Chase-Lev deques + bitmask steal).
//! Alternative: Shared (single StealQueue).

pub(crate) mod cps;
mod exec;
pub(crate) mod infra;
pub mod pool;

pub(crate) use infra::{arena, cont_arena, deque, eventcount};
pub(crate) use cps::{cont, chain, walk};
pub(crate) use exec::{view, worker};
pub use exec::{run, queue};

use std::marker::PhantomData;
use crate::ops::LiftOps;
use crate::domain::Domain;
use super::super::Executor;
use pool::FunnelPool;
use queue::{WorkStealing, per_worker::PerWorker};

/// How multi-child nodes accumulate child results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccumulateMode {
    OnArrival,
    OnFinalize,
}

pub struct HyloFunnelSpec<W: WorkStealing = PerWorker> {
    pub accumulate: AccumulateMode,
    pub queue: W::Spec,
}

impl HyloFunnelSpec {
    /// Default: OnArrival + PerWorker with default deque capacity.
    pub fn default_for(_n_workers: usize) -> Self {
        HyloFunnelSpec {
            accumulate: AccumulateMode::OnArrival,
            queue: Default::default(),
        }
    }

    /// PerWorker strategy with custom AccumulateMode.
    pub fn per_worker(accumulate: AccumulateMode) -> Self {
        HyloFunnelSpec { accumulate, queue: Default::default() }
    }
}

impl HyloFunnelSpec<queue::Shared> {
    /// Shared strategy with custom AccumulateMode.
    pub fn shared(accumulate: AccumulateMode) -> Self {
        HyloFunnelSpec { accumulate, queue: Default::default() }
    }
}

impl<W: WorkStealing> HyloFunnelSpec<W> {
    pub fn new(accumulate: AccumulateMode, queue: W::Spec) -> Self {
        HyloFunnelSpec { accumulate, queue }
    }
}

pub struct HyloFunnelIn<D, W: WorkStealing = PerWorker> {
    pool: FunnelPool,
    spec: HyloFunnelSpec<W>,
    _domain: PhantomData<D>,
}

impl<D, W: WorkStealing> HyloFunnelIn<D, W> {
    pub fn new(n_workers: usize, spec: HyloFunnelSpec<W>) -> Self {
        HyloFunnelIn { pool: FunnelPool::new(n_workers), spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>, W: WorkStealing> Executor<N, R, D> for HyloFunnelIn<D, W>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        run::run_fold::<_, _, _, _, _, W>(fold, graph, root, &self.pool, self.spec.accumulate, &self.spec.queue)
    }
}

impl<D, W: WorkStealing> HyloFunnelIn<D, W> {
    pub fn run<N, H, R>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N>, N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static {
        run::run_fold::<_, _, _, _, _, W>(fold, graph, root, &self.pool, self.spec.accumulate, &self.spec.queue)
    }

    pub fn run_lifted<N, R, N0, H0, R0, H>(
        &self, lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>, graph: &<D as Domain<N0>>::Treeish, root: &N0,
    ) -> R0 where
        D: Domain<N> + Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone, <D as Domain<N0>>::Treeish: Clone,
        N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static,
        N0: Clone + Send + 'static, H0: 'static, R0: 'static,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }
}

#[cfg(test)]
mod tests;
