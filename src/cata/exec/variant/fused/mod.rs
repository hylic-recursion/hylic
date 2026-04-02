//! Fused executor: zero-overhead sequential recursive traversal.
//!
//! Supports ALL domains — it never clones the fold or graph.

use std::marker::PhantomData;
use crate::ops::{FoldOps, TreeOps, LiftOps};
use crate::domain::Domain;
use super::super::Executor;

/// Fused sequential executor, parameterized by domain.
pub struct FusedIn<D>(pub(crate) PhantomData<D>);

impl<D> Clone for FusedIn<D> { fn clone(&self) -> Self { *self } }
impl<D> Copy for FusedIn<D> {}
impl<D> std::fmt::Debug for FusedIn<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Fused") }
}

// ── Trait impl (for generic code: pipeline, &impl Executor) ──

impl<N: 'static, R: 'static, D: Domain<N>> Executor<N, R, D> for FusedIn<D> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        recurse(fold, graph, root)
    }
}

// ── Inherent methods (no trait import needed at call sites) ──

impl<D> FusedIn<D> {
    pub fn run<N: 'static, H: 'static, R: 'static>(
        &self,
        fold: &<D as Domain<N>>::Fold<H, R>,
        graph: &<D as Domain<N>>::Treeish,
        root: &N,
    ) -> R
    where D: Domain<N>
    {
        recurse(fold, graph, root)
    }

    pub fn run_lifted<N: 'static, R: 'static, N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> R0
    where
        D: Domain<N> + Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }

    pub fn run_lifted_zipped<N: 'static, R: Clone + 'static, N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> (R0, R)
    where
        D: Domain<N> + Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        let inner = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        (lift.unwrap(inner.clone()), inner)
    }
}

// ── Recursion engine ──────────────────────────────

// ANCHOR: run_inner
fn recurse<N, H, R>(
    fold: &impl FoldOps<N, H, R>,
    graph: &impl TreeOps<N>,
    node: &N,
) -> R {
    let mut heap = fold.init(node);
    graph.visit(node, &mut |child: &N| {
        let r = recurse(fold, graph, child);
        fold.accumulate(&mut heap, &r);
    });
    fold.finalize(&heap)
}
// ANCHOR_END: run_inner
