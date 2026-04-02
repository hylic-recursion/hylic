//! Pool executor: parallel child recursion via our own WorkPool.
//!
//! Currently Shared-only pending SyncRef for domain-generic support.

use std::sync::Arc;
use crate::ops::{FoldOps, TreeOps, LiftOps};
use crate::domain::Shared;
use crate::prelude::parallel::pool::{WorkPool, fork_join_map};
use super::super::Executor;

/// Fork-join policy for the pool executor.
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

    pub fn with_fork_depth(mut self, depth: usize) -> Self {
        self.fork_depth_limit = depth;
        self
    }

    pub fn with_min_children(mut self, min: usize) -> Self {
        self.min_children_to_fork = min;
        self
    }
}

/// Parallel executor backed by a WorkPool with tree-aware fork-join.
pub struct PoolIn {
    pool: Arc<WorkPool>,
    spec: PoolSpec,
}

impl PoolIn {
    pub fn new(pool: &Arc<WorkPool>, spec: PoolSpec) -> Self {
        PoolIn { pool: pool.clone(), spec }
    }
}

// ── Trait impl ────────────────────────────────────

impl<N, R> Executor<N, R, Shared> for PoolIn
where N: Clone + Send + Sync + 'static, R: Send + Sync + 'static,
{
    fn run<H: 'static>(
        &self,
        fold: &<Shared as crate::domain::Domain<N>>::Fold<H, R>,
        graph: &<Shared as crate::domain::Domain<N>>::Treeish,
        root: &N,
    ) -> R {
        pool_recurse(fold, graph, root, &self.pool, &self.spec, 0)
    }
}

// ── Inherent methods ──────────────────────────────

impl PoolIn {
    pub fn run<N: Clone + Send + Sync + 'static, H: 'static, R: Send + Sync + 'static>(
        &self,
        fold: &<Shared as crate::domain::Domain<N>>::Fold<H, R>,
        graph: &<Shared as crate::domain::Domain<N>>::Treeish,
        root: &N,
    ) -> R {
        pool_recurse(fold, graph, root, &self.pool, &self.spec, 0)
    }

    pub fn run_lifted<N: Clone + Send + Sync + 'static, R: Send + Sync + 'static, N0: Clone + Send + Sync + 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<Shared, N0, H0, R0, N, H, R>,
        fold: &<Shared as crate::domain::Domain<N0>>::Fold<H0, R0>,
        graph: &<Shared as crate::domain::Domain<N0>>::Treeish,
        root: &N0,
    ) -> R0
    where
        <Shared as crate::domain::Domain<N0>>::Fold<H0, R0>: Clone,
        <Shared as crate::domain::Domain<N0>>::Treeish: Clone,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }

    pub fn run_lifted_zipped<N: Clone + Send + Sync + 'static, R: Clone + Send + Sync + 'static, N0: Clone + Send + Sync + 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<Shared, N0, H0, R0, N, H, R>,
        fold: &<Shared as crate::domain::Domain<N0>>::Fold<H0, R0>,
        graph: &<Shared as crate::domain::Domain<N0>>::Treeish,
        root: &N0,
    ) -> (R0, R)
    where
        <Shared as crate::domain::Domain<N0>>::Fold<H0, R0>: Clone,
        <Shared as crate::domain::Domain<N0>>::Treeish: Clone,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        let inner = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        (lift.unwrap(inner.clone()), inner)
    }
}

// ── Recursion engine ──────────────────────────────

fn pool_recurse<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + Sync),
    graph: &(impl TreeOps<N> + Sync),
    node: &N,
    pool: &WorkPool,
    spec: &PoolSpec,
    depth: usize,
) -> R
where N: Clone + Send + Sync, R: Send + Sync,
{
    let mut heap = fold.init(node);
    let children = graph.apply(node);

    let should_fork = depth < spec.fork_depth_limit
        && children.len() >= spec.min_children_to_fork;

    if should_fork {
        let results = fork_join_map(
            pool,
            &children,
            &|child| pool_recurse(fold, graph, child, pool, spec, depth + 1),
            0,
            spec.fork_depth_limit.saturating_sub(depth),
        );
        for r in &results { fold.accumulate(&mut heap, r); }
    } else {
        for child in &children {
            let r = pool_recurse(fold, graph, child, pool, spec, depth + 1);
            fold.accumulate(&mut heap, &r);
        }
    }

    fold.finalize(&heap)
}
