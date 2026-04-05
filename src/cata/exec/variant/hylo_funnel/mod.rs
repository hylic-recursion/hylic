//! Hylo-funnel: bare-metal CPS parallel hylomorphism executor.
//!
//! Everything is data, nothing is a closure:
//! - Continuations: Cont<H, R> enum (Root, Direct, Slot)
//! - Tasks: FunnelTask<N, H, R> enum (Walk, Rake)
//! - Workers pattern-match on both. No Box, no type erasure.
//!
//! Per-worker Chase-Lev deques. Push/pop is local (no atomic).
//! Steal is a rare CAS from a neighbor. EventCount for parking.
//! Arena-allocated ChainNodes + ContArena for continuations.

mod deque;
mod eventcount;
mod arena;
mod cont_arena;
pub(crate) mod fold_chain;
mod pool;
mod walk;

use std::marker::PhantomData;
use crate::ops::LiftOps;
use crate::domain::Domain;
use super::super::Executor;

pub struct HyloFunnelSpec { pub _reserved: () }
impl HyloFunnelSpec {
    pub fn default_for(_n_workers: usize) -> Self { HyloFunnelSpec { _reserved: () } }
}

pub struct HyloFunnelIn<D> {
    n_workers: usize,
    _spec: HyloFunnelSpec,
    _domain: PhantomData<D>,
}

impl<D> HyloFunnelIn<D> {
    pub fn new(n_workers: usize, spec: HyloFunnelSpec) -> Self {
        HyloFunnelIn { n_workers, _spec: spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>> Executor<N, R, D> for HyloFunnelIn<D>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        walk::run_fold(fold, graph, root, self.n_workers)
    }
}

impl<D> HyloFunnelIn<D> {
    pub fn run<N, H, R>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N>, N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static {
        walk::run_fold(fold, graph, root, self.n_workers)
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
