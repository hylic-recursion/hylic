//! Hylo-funnel: bare-metal CPS parallel hylomorphism executor.
//!
//! Generic over P: FunnelPolicy, which bundles three behavioral axes:
//! - Queue topology (PerWorker, Shared)
//! - Accumulation strategy (OnArrival, OnFinalize)
//! - Wake policy (EveryPush, OncePerBatch, EveryK)
//!
//! All axes are monomorphized. Zero runtime overhead for strategy dispatch.

pub(crate) mod cps;
mod exec;
pub(crate) mod infra;
pub mod pool;

pub(crate) use infra::{arena, cont_arena, deque, eventcount};
pub(crate) use cps::{cont, chain, walk};
pub(crate) use exec::{view, worker};
pub use exec::{queue, accumulate, wake, policy};
pub(crate) use exec::run;

use std::marker::PhantomData;
use crate::ops::LiftOps;
use crate::domain::Domain;
use super::super::Executor;
use pool::FunnelPool;
use policy::FunnelPolicy;
use queue::WorkStealing;
use wake::WakeStrategy;

pub struct Spec<P: FunnelPolicy = policy::Default> {
    pub n_workers: usize,
    pub queue: <P::Queue as WorkStealing>::Spec,
    pub wake: <P::Wake as WakeStrategy>::Spec,
}

impl Spec<policy::Default> {
    pub fn default(n_workers: usize) -> Self {
        Spec { n_workers, queue: Default::default(), wake: Default::default() }
    }
}

impl Spec<policy::GraphHeavy> {
    pub fn for_graph_heavy(n_workers: usize) -> Self {
        Spec { n_workers, queue: Default::default(), wake: Default::default() }
    }
}

impl Spec<policy::WideLight> {
    pub fn for_wide_light(n_workers: usize) -> Self {
        Spec { n_workers, queue: Default::default(), wake: Default::default() }
    }
}

impl Spec<policy::LowOverhead> {
    pub fn for_low_overhead(n_workers: usize) -> Self {
        Spec { n_workers, queue: Default::default(), wake: Default::default() }
    }
}

impl<P: FunnelPolicy> Spec<P> {
    pub fn new(n_workers: usize, queue: <P::Queue as WorkStealing>::Spec, wake: <P::Wake as WakeStrategy>::Spec) -> Self {
        Spec { n_workers, queue, wake }
    }
}

pub struct Exec<D, P: FunnelPolicy = policy::Default> {
    pool: FunnelPool,
    spec: Spec<P>,
    _domain: PhantomData<D>,
}

impl<D, P: FunnelPolicy> Exec<D, P> {
    pub fn new(spec: Spec<P>) -> Self {
        Exec { pool: FunnelPool::new(spec.n_workers), spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>, P: FunnelPolicy> Executor<N, R, D> for Exec<D, P>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        run::run_fold::<_, _, _, _, _, P>(fold, graph, root, &self.pool, &self.spec.queue, &self.spec.wake)
    }
}

impl<D, P: FunnelPolicy> Exec<D, P> {
    pub fn run<N, H, R>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N>, N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static {
        run::run_fold::<_, _, _, _, _, P>(fold, graph, root, &self.pool, &self.spec.queue, &self.spec.wake)
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
