//! Sequential executor: collect children to Vec, iterate.
//!
//! Supports ALL domains — it borrows fold/graph, never clones them.

use std::marker::PhantomData;
use crate::ops::{FoldOps, TreeOps, LiftOps};
use crate::domain::Domain;
use super::super::Executor;

/// Unfused sequential executor, parameterized by domain.
pub struct SequentialIn<D>(pub(crate) PhantomData<D>);

impl<D> Clone for SequentialIn<D> { fn clone(&self) -> Self { *self } }
impl<D> Copy for SequentialIn<D> {}
impl<D> std::fmt::Debug for SequentialIn<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Sequential") }
}

impl<N: Clone + 'static, R: 'static, D: Domain<N>> Executor<N, R, D> for SequentialIn<D> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        recurse(fold, graph, root)
    }
}

impl<D> SequentialIn<D> {
    pub fn run<N: Clone + 'static, H: 'static, R: 'static>(
        &self,
        fold: &<D as Domain<N>>::Fold<H, R>,
        graph: &<D as Domain<N>>::Treeish,
        root: &N,
    ) -> R
    where D: Domain<N>
    {
        recurse(fold, graph, root)
    }

    pub fn run_lifted<N: Clone + 'static, R: 'static, N0: Clone + 'static, H0: 'static, R0: 'static, H: 'static>(
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

    pub fn run_lifted_zipped<N: Clone + 'static, R: Clone + 'static, N0: Clone + 'static, H0: 'static, R0: 'static, H: 'static>(
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

fn recurse<N: Clone, H, R>(
    fold: &impl FoldOps<N, H, R>,
    graph: &impl TreeOps<N>,
    node: &N,
) -> R {
    let mut heap = fold.init(node);
    for child in graph.apply(node) {
        fold.accumulate(&mut heap, &recurse(fold, graph, &child));
    }
    fold.finalize(&heap)
}
