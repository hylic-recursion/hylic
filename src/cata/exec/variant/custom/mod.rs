//! Custom child-visiting executor — escape hatch.
//!
//! For user-defined traversal strategies that don't fit the built-in
//! executors (Fused, Sequential, Rayon). Wraps a ChildVisitorFn that
//! controls how children are traversed and results delivered.
//!
//! Pays 5 Arc clones per node in recursion (fold×3, graph, visitor).
//! Prefer the built-in zero-sized variants when possible.

use std::sync::Arc;
use crate::fold::Fold;
use crate::graph::Treeish;
use super::super::Executor;

/// Visitor function controlling child traversal and parallelism.
pub type ChildVisitorFn<N, R> = dyn Fn(
    &Treeish<N>,
    &N,
    &(dyn Fn(&N) -> R + Send + Sync),
    &mut dyn FnMut(&R),
) + Send + Sync;

/// Custom executor backed by a user-provided `ChildVisitorFn`.
pub struct Custom<N, R> {
    visitor: Arc<ChildVisitorFn<N, R>>,
}

impl<N, R> Custom<N, R> {
    pub fn new(visitor: Arc<ChildVisitorFn<N, R>>) -> Self {
        Custom { visitor }
    }
}

impl<N: 'static, R: 'static> Executor<N, R> for Custom<N, R> {
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        recurse(&self.visitor, fold, graph, root)
    }
}

fn recurse<N: 'static, H: 'static, R: 'static>(
    vc: &Arc<ChildVisitorFn<N, R>>,
    fold: &Fold<N, H, R>,
    graph: &Treeish<N>,
    node: &N,
) -> R {
    let vc_c = vc.clone();
    let f_c = fold.clone();
    let g_c = graph.clone();
    let recurse_fn = move |child: &N| -> R {
        recurse(&vc_c, &f_c, &g_c, child)
    };

    let mut heap = fold.init(node);
    vc(graph, node, &recurse_fn, &mut |r: &R| fold.accumulate(&mut heap, r));
    fold.finalize(&heap)
}
