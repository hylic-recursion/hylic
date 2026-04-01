//! Fused executor: zero-overhead sequential recursive traversal.
//!
//! Supports ALL domains — it never clones the fold or graph.

use std::marker::PhantomData;
use crate::ops::{FoldOps, TreeOps};
use crate::domain::Domain;
use super::super::Executor;

/// Fused sequential executor, parameterized by domain.
pub struct FusedIn<D>(pub(crate) PhantomData<D>);

impl<D> Clone for FusedIn<D> { fn clone(&self) -> Self { *self } }
impl<D> Copy for FusedIn<D> {}
impl<D> std::fmt::Debug for FusedIn<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Fused") }
}

impl<N: 'static, R: 'static, D: Domain<N>> Executor<N, R, D> for FusedIn<D> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        recurse(fold, graph, root)
    }
}

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
