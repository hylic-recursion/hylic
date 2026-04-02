//! Pool executor: parallel child recursion via our own WorkPool.
//!
//! Domain-generic — works with Shared, Local, and Owned via SyncRef.
//! Uses binary-split fork-join (fork_join_map) with depth-based
//! sequential cutoff. No rayon dependency.
//!
//! Currently Shared-only pending SyncRef implementation.

use std::sync::Arc;
use crate::ops::{FoldOps, TreeOps};
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

// Shared impl — will become domain-generic (PoolIn<D>) with SyncRef.
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
