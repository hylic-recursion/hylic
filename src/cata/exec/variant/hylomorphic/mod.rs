//! Hylomorphic parallel executor: CPS zipper with reactive accumulation.

pub(crate) mod fold_chain;
mod walk;
#[cfg(test)]
mod tests;

use std::marker::PhantomData;
use std::sync::Arc;
use crate::ops::LiftOps;
use crate::domain::Domain;
use crate::prelude::parallel::pool::{WorkPool, PoolExecView};
use super::super::Executor;

pub struct HylomorphicSpec { pub _reserved: () }
impl HylomorphicSpec {
    pub fn default_for(_n_workers: usize) -> Self { HylomorphicSpec { _reserved: () } }
}

pub struct HylomorphicIn<D> {
    pool: Arc<WorkPool>,
    _spec: HylomorphicSpec,
    _domain: PhantomData<D>,
}

impl<D> HylomorphicIn<D> {
    pub fn new(pool: &Arc<WorkPool>, spec: HylomorphicSpec) -> Self {
        HylomorphicIn { pool: pool.clone(), _spec: spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>> Executor<N, R, D> for HylomorphicIn<D>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        let view = PoolExecView::new(&self.pool);
        walk::run_fold(fold, graph, root, &view)
    }
}

impl<D> HylomorphicIn<D> {
    pub fn run<N, H, R>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N>, N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static {
        let view = PoolExecView::new(&self.pool);
        walk::run_fold(fold, graph, root, &view)
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

