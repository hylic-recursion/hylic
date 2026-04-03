//! Pool executor: parallel child recursion via WorkPool + PoolExecView.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::ops::{FoldOps, TreeOps, LiftOps};
use crate::domain::Domain;
use crate::prelude::parallel::pool::{WorkPool, PoolExecView, SyncRef, fork_join_map};
use super::super::Executor;

pub struct PoolSpec {
    pub fork_depth_limit: usize,
    pub min_children_to_fork: usize,
}

impl PoolSpec {
    pub fn default_for(n_workers: usize) -> Self {
        PoolSpec {
            fork_depth_limit: (n_workers as f64).log2().ceil() as usize + 2,
            min_children_to_fork: 2,
        }
    }
    pub fn with_fork_depth(mut self, depth: usize) -> Self { self.fork_depth_limit = depth; self }
    pub fn with_min_children(mut self, min: usize) -> Self { self.min_children_to_fork = min; self }
}

pub struct PoolIn<D> {
    pool: Arc<WorkPool>,
    spec: PoolSpec,
    _domain: PhantomData<D>,
}

impl<D> PoolIn<D> {
    pub fn new(pool: &Arc<WorkPool>, spec: PoolSpec) -> Self {
        PoolIn { pool: pool.clone(), spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>> Executor<N, R, D> for PoolIn<D>
where N: Clone + Send + 'static, R: Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        let view = PoolExecView::new(&self.pool);
        pool_recurse(&SyncRef(fold), &SyncRef(graph), root, &view, &self.spec, 0)
    }
}

impl<D> PoolIn<D> {
    pub fn run<N: Clone + Send + 'static, H: 'static, R: Send + 'static>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N> {
        let view = PoolExecView::new(&self.pool);
        pool_recurse(&SyncRef(fold), &SyncRef(graph), root, &view, &self.spec, 0)
    }

    pub fn run_lifted<N: Clone + Send + 'static, R: Send + 'static, N0: Clone + Send + 'static, H0: 'static, R0: 'static, H: 'static>(
        &self, lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>, graph: &<D as Domain<N0>>::Treeish, root: &N0,
    ) -> R0 where D: Domain<N> + Domain<N0>, <D as Domain<N0>>::Fold<H0, R0>: Clone, <D as Domain<N0>>::Treeish: Clone {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }

    pub fn run_lifted_zipped<N: Clone + Send + 'static, R: Clone + Send + 'static, N0: Clone + Send + 'static, H0: 'static, R0: 'static, H: 'static>(
        &self, lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>, graph: &<D as Domain<N0>>::Treeish, root: &N0,
    ) -> (R0, R) where D: Domain<N> + Domain<N0>, <D as Domain<N0>>::Fold<H0, R0>: Clone, <D as Domain<N0>>::Treeish: Clone {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        let inner = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        (lift.unwrap(inner.clone()), inner)
    }
}

fn pool_recurse<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    node: &N, view: &PoolExecView, spec: &PoolSpec, depth: usize,
) -> R where N: Clone + Send, R: Send {
    let mut heap = fold.init(node);
    let children = graph.apply(node);
    let should_fork = depth < spec.fork_depth_limit && children.len() >= spec.min_children_to_fork;
    if should_fork {
        let results = fork_join_map(
            view, &children,
            &|child| pool_recurse(fold, graph, child, view, spec, depth + 1),
            0, spec.fork_depth_limit.saturating_sub(depth),
        );
        for r in &results { fold.accumulate(&mut heap, r); }
    } else {
        for child in &children {
            let r = pool_recurse(fold, graph, child, view, spec, depth + 1);
            fold.accumulate(&mut heap, &r);
        }
    }
    fold.finalize(&heap)
}
