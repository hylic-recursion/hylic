//! Fused executor: zero-overhead sequential recursive traversal.
//! Supports ALL domains and ALL graph types.

use crate::ops::{FoldOps, TreeOps};
use crate::domain::Domain;
use super::super::{Executor, ExecutorSpec};

pub struct Spec;

impl Clone for Spec { fn clone(&self) -> Self { *self } }
impl Copy for Spec {}
impl std::fmt::Debug for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Fused") }
}

impl ExecutorSpec for Spec {
    type Resource<'r> = ();
    type Session<'s> = Self;
    fn attach(self, _: Self::Resource<'_>) -> Self::Session<'_> { self }
    fn with_session<R>(&self, f: impl for<'s> FnOnce(&Self) -> R) -> R { f(self) }
}

impl<N: 'static, R: 'static, D: Domain<N>, G: TreeOps<N> + 'static> Executor<N, R, D, G> for Spec {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &G, root: &N) -> R {
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
