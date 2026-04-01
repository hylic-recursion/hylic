//! Sequential executor: collect children, iterate.
//!
//! Like Fused but uses TreeOps::apply (Vec collection) instead of
//! TreeOps::visit (callback). Exists as a reference for the unfused
//! traversal pattern. Prefer Fused for performance.
//!
//! No Send, Sync, or Arc in this module.

use crate::fold::FoldOps;
use crate::graph::types::TreeOps;
use super::super::Executor;

/// Unfused sequential executor.
#[derive(Clone, Copy, Debug)]
pub struct Sequential;

impl<N: Clone + 'static, R: 'static> Executor<N, R> for Sequential {
    fn run<H: 'static>(
        &self,
        fold: &(impl FoldOps<N, H, R> + ?Sized),
        graph: &(impl TreeOps<N> + ?Sized),
        root: &N,
    ) -> R {
        recurse(fold, graph, root)
    }
}

fn recurse<N: Clone, H, R>(
    fold: &(impl FoldOps<N, H, R> + ?Sized),
    graph: &(impl TreeOps<N> + ?Sized),
    node: &N,
) -> R {
    let mut heap = fold.init(node);
    for child in graph.apply(node) {
        fold.accumulate(&mut heap, &recurse(fold, graph, &child));
    }
    fold.finalize(&heap)
}
