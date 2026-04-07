//! Rayon executor: parallel child recursion via rayon's par_iter.
//!
//! Shared domain only — requires Sync on fold/graph references.

use std::marker::PhantomData;
use crate::ops::{FoldOps, TreeOps, LiftOps};
use crate::domain::Shared;
use super::super::Executor;

pub struct Spec;

/// Parallel executor via rayon's work-stealing thread pool.
pub struct Exec<D>(pub(crate) PhantomData<D>);

impl<D> Exec<D> {
    pub fn from_spec(_spec: Spec) -> Self { Exec(PhantomData) }
}

impl<D> Clone for Exec<D> { fn clone(&self) -> Self { *self } }
impl<D> Copy for Exec<D> {}
impl<D> std::fmt::Debug for Exec<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Rayon") }
}

impl<N, R> Executor<N, R, Shared> for Exec<Shared>
where N: Clone + Send + Sync + 'static, R: Send + Sync + 'static,
{
    fn run<H: 'static>(
        &self,
        fold: &<Shared as crate::domain::Domain<N>>::Fold<H, R>,
        graph: &<Shared as crate::domain::Domain<N>>::Treeish,
        root: &N,
    ) -> R {
        recurse(fold, graph, root)
    }
}

impl Exec<Shared> {
    pub fn run<N: Clone + Send + Sync + 'static, H: 'static, R: Send + Sync + 'static>(
        &self,
        fold: &<Shared as crate::domain::Domain<N>>::Fold<H, R>,
        graph: &<Shared as crate::domain::Domain<N>>::Treeish,
        root: &N,
    ) -> R {
        recurse(fold, graph, root)
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

fn recurse<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + Sync),
    graph: &(impl TreeOps<N> + Sync),
    node: &N,
) -> R
where N: Clone + Send + Sync, R: Send + Sync,
{
    use rayon::prelude::*;

    let mut heap = fold.init(node);
    let children = graph.apply(node);

    if children.len() <= 1 {
        for child in &children {
            fold.accumulate(&mut heap, &recurse(fold, graph, child));
        }
    } else {
        let results: Vec<R> = children.par_iter()
            .map(|c| recurse(fold, graph, c))
            .collect();
        for r in &results { fold.accumulate(&mut heap, r); }
    }

    fold.finalize(&heap)
}
