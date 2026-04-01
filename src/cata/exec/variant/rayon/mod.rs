//! Rayon executor: parallel child recursion via rayon's par_iter.
//!
//! Shared domain only — requires Sync on fold/graph references.

use std::marker::PhantomData;
use crate::ops::{FoldOps, TreeOps};
use crate::domain::Shared;
use super::super::Executor;

/// Parallel executor via rayon's work-stealing thread pool.
pub struct RayonIn<D>(pub(crate) PhantomData<D>);

impl<D> Clone for RayonIn<D> { fn clone(&self) -> Self { *self } }
impl<D> Copy for RayonIn<D> {}
impl<D> std::fmt::Debug for RayonIn<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Rayon") }
}

impl<N, R> Executor<N, R, Shared> for RayonIn<Shared>
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
