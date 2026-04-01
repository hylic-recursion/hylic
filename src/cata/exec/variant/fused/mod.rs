//! Fused executor: zero-overhead sequential recursive traversal.
//!
//! Recursion and accumulation interleave inside `graph.visit` —
//! no collection, no allocation, no Arc clones. The fold and graph
//! are passed by reference through the entire recursion.

use crate::fold::Fold;
use crate::graph::Treeish;
use super::super::Executor;

/// Fused sequential executor.
///
/// The simplest, fastest executor for single-threaded use.
/// No thread boundary is ever crossed — Send, Sync, Arc are
/// absent from this module entirely.
#[derive(Clone, Copy, Debug)]
pub struct Fused;

impl<N: 'static, R: 'static> Executor<N, R> for Fused {
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        recurse(fold, graph, root)
    }
}

fn recurse<N: 'static, H: 'static, R: 'static>(
    fold: &Fold<N, H, R>,
    graph: &Treeish<N>,
    node: &N,
) -> R {
    let mut heap = fold.init(node);
    graph.visit(node, &mut |child: &N| {
        let r = recurse(fold, graph, child);
        fold.accumulate(&mut heap, &r);
    });
    fold.finalize(&heap)
}
