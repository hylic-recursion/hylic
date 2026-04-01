//! Custom child-visiting executor — Shared-domain escape hatch.
//!
//! For user-defined traversal strategies. Wraps a ChildVisitorFn
//! that controls child traversal. Shared-only because the recurse
//! closure captures cloned fold/graph/visitor (requires Clone +
//! Send+Sync, which Arc-based Shared types provide).
//!
//! For other domains, implement Executor directly — it's one method.

use std::sync::Arc;
use crate::fold::Fold;
use crate::graph::Treeish;
use crate::domain::Shared;
use super::super::Executor;

/// Visitor function controlling child traversal.
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

impl<N: 'static, R: 'static> Executor<N, R, Shared> for Custom<N, R> {
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
