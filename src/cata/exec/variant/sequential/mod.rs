//! Sequential executor: collect children, iterate.
//!
//! Like Fused but uses `graph.apply()` (Vec collection) instead of
//! `graph.visit()` (callback). Exists as a reference for the unfused
//! traversal pattern. Prefer Fused for performance.
//!
//! No Send, Sync, or Arc in this module.

use crate::fold::Fold;
use crate::graph::Treeish;
use super::super::Executor;

/// Unfused sequential executor.
#[derive(Clone, Copy, Debug)]
pub struct Sequential;

impl<N: Clone + 'static, R: 'static> Executor<N, R> for Sequential {
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        recurse(fold, graph, root)
    }
}

fn recurse<N: Clone + 'static, H: 'static, R: 'static>(
    fold: &Fold<N, H, R>,
    graph: &Treeish<N>,
    node: &N,
) -> R {
    let mut heap = fold.init(node);
    for child in graph.apply(node) {
        fold.accumulate(&mut heap, &recurse(fold, graph, &child));
    }
    fold.finalize(&heap)
}
