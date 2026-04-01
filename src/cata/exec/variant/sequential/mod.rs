//! Sequential executor: collect children to Vec, iterate.
//!
//! Supports ALL domains — it borrows fold/graph, never clones them.

use std::marker::PhantomData;
use crate::ops::{FoldOps, TreeOps};
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
