//! Fused-parallel executor: visit-based DFS with on-demand forking.
//!
//! Combines Fused's zero-allocation callback traversal with Pool's
//! work-stealing parallelism. Children are processed one at a time
//! via `visit_inline` (like Fused). When idle workers are detected,
//! remaining children are buffered and forked via `fork_join_map`.
//!
//! Hot path (no forking): identical to Fused — zero overhead.
//! Cold path (forking): remaining children cloned into a Vec, then
//! distributed to workers. The Vec cost is paid ONLY when parallelism
//! is actually beneficial, not at every node.
//!
//! Domain-generic via SyncRef.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::ops::{FoldOps, TreeOps, LiftOps};
use crate::domain::Domain;
use crate::prelude::parallel::pool::{WorkPool, PoolExecView, SyncRef, fork_join_map};
use super::super::Executor;

/// Policy for the fused-parallel executor.
pub struct FusedParallelSpec {
    /// Minimum depth before forking is considered. Prevents forking at
    /// the root (where the graph function is cheapest).
    pub min_depth_to_fork: usize,
    /// Maximum number of children to process sequentially before checking
    /// for fork demand. Lower = more responsive, higher = less overhead.
    pub check_interval: usize,
}

impl FusedParallelSpec {
    pub fn default_for(_n_workers: usize) -> Self {
        FusedParallelSpec {
            min_depth_to_fork: 1,
            check_interval: 1,
        }
    }
}

pub struct FusedParallelIn<D> {
    pool: Arc<WorkPool>,
    spec: FusedParallelSpec,
    _domain: PhantomData<D>,
}

impl<D> FusedParallelIn<D> {
    pub fn new(pool: &Arc<WorkPool>, spec: FusedParallelSpec) -> Self {
        FusedParallelIn { pool: pool.clone(), spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>> Executor<N, R, D> for FusedParallelIn<D>
where N: Clone + Send + 'static, R: Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        let view = PoolExecView::new(&self.pool);
        fused_par_recurse(&SyncRef(fold), &SyncRef(graph), root, &view, &self.spec, 0)
    }
}

impl<D> FusedParallelIn<D> {
    pub fn run<N: Clone + Send + 'static, H: 'static, R: Send + 'static>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N> {
        let view = PoolExecView::new(&self.pool);
        fused_par_recurse(&SyncRef(fold), &SyncRef(graph), root, &view, &self.spec, 0)
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

// ── Recursion engine ──────────────────────────────

fn fused_par_recurse<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    node: &N,
    view: &PoolExecView,
    spec: &FusedParallelSpec,
    depth: usize,
) -> R
where N: Clone + Send, R: Send,
{
    let mut heap = fold.init(node);

    // Track children visited so far and whether we've switched to fork mode.
    let mut child_count = 0usize;
    let mut forked_children: Option<Vec<N>> = None;

    graph.visit_inline(node, &mut |child: &N| {
        if let Some(ref mut buf) = forked_children {
            // Fork mode: buffer remaining children for parallel processing.
            buf.push(child.clone());
            return;
        }

        // Sequential mode: recurse immediately (like Fused).
        let r = fused_par_recurse(fold, graph, child, view, spec, depth + 1);
        fold.accumulate(&mut heap, &r);
        child_count += 1;

        // Check if we should switch to fork mode.
        // Conditions: deep enough, processed enough children sequentially,
        // and workers appear idle (StealQueue is empty).
        if depth >= spec.min_depth_to_fork
            && child_count >= spec.check_interval
            && view.queue_is_empty()
        {
            // Switch to fork mode. Next children from visit will be buffered.
            forked_children = Some(Vec::new());
        }
    });

    // If we buffered children for forking, process them in parallel.
    if let Some(children) = forked_children {
        if !children.is_empty() {
            let results = fork_join_map(
                view, &children,
                &|child| fused_par_recurse(fold, graph, child, view, spec, depth + 1),
                0, 8,  // max binary-split depth for the forked batch
            );
            for r in &results {
                fold.accumulate(&mut heap, r);
            }
        }
    }

    fold.finalize(&heap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::shared as dom;
    use crate::prelude::{WorkPool, WorkPoolSpec};

    #[derive(Clone)]
    struct N { val: i32, children: Vec<N> }

    fn big_tree(n: usize, bf: usize) -> N {
        fn build(id: &mut i32, remaining: &mut usize, bf: usize) -> N {
            let val = *id; *id += 1; *remaining = remaining.saturating_sub(1);
            let mut ch = Vec::new();
            for _ in 0..bf { if *remaining == 0 { break; } ch.push(build(id, remaining, bf)); }
            N { val, children: ch }
        }
        let mut id = 1; let mut remaining = n;
        build(&mut id, &mut remaining, bf)
    }

    #[test]
    fn matches_fused() {
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);

        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = FusedParallelIn::<crate::domain::Shared>::new(pool, FusedParallelSpec::default_for(3));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn matches_fused_200_nodes() {
        let tree = big_tree(200, 6);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);

        WorkPool::with(WorkPoolSpec::threads(4), |pool| {
            let exec = FusedParallelIn::<crate::domain::Shared>::new(pool, FusedParallelSpec::default_for(4));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn zero_workers_matches_fused() {
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);

        WorkPool::with(WorkPoolSpec::threads(0), |pool| {
            let exec = FusedParallelIn::<crate::domain::Shared>::new(pool, FusedParallelSpec::default_for(0));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn stress_20_iterations() {
        let tree = big_tree(200, 6);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);

        for i in 0..20 {
            WorkPool::with(WorkPoolSpec::threads(4), |pool| {
                let exec = FusedParallelIn::<crate::domain::Shared>::new(pool, FusedParallelSpec::default_for(4));
                assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
            });
        }
    }

    #[test]
    fn with_lift() {
        use crate::prelude::{ParLazy, ParEager, EagerSpec};

        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);

        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = FusedParallelIn::<crate::domain::Shared>::new(pool, FusedParallelSpec::default_for(3));

            assert_eq!(exec.run_lifted(&ParLazy::lift(pool), &fold, &graph, &tree), expected, "Lazy+FusedPar");
            assert_eq!(exec.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &fold, &graph, &tree), expected, "Eager+FusedPar");
        });
    }

    #[test]
    fn local_domain() {
        use crate::domain::local;

        let tree = big_tree(60, 4);
        let fold = local::fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; }, |h: &i32| *h);
        let graph = local::treeish_visit(|n: &N, cb: &mut dyn FnMut(&N)| {
            for c in &n.children { cb(c); }
        });
        let expected = local::FUSED.run(&fold, &graph, &tree);

        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = FusedParallelIn::<crate::domain::Local>::new(pool, FusedParallelSpec::default_for(3));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }
}
