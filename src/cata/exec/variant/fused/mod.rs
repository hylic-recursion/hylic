//! Fused executor: zero-overhead sequential recursive traversal.
//!
//! Recursion and accumulation interleave inside the tree visit callback —
//! no collection, no allocation, no Arc clones. Fold and graph are
//! accessed only through the operations traits (FoldOps, TreeOps).
//!
//! No Send, Sync, or Arc in this module.

use crate::fold::FoldOps;
use crate::graph::types::TreeOps;
use super::super::Executor;

/// Fused sequential executor.
///
/// The simplest, fastest executor for single-threaded use.
/// No thread boundary is ever crossed.
#[derive(Clone, Copy, Debug)]
pub struct Fused;

impl<N: 'static, R: 'static> Executor<N, R> for Fused {
    fn run<H: 'static>(
        &self,
        fold: &(impl FoldOps<N, H, R> + ?Sized),
        graph: &(impl TreeOps<N> + ?Sized),
        root: &N,
    ) -> R {
        recurse(fold, graph, root)
    }
}

// ANCHOR: run_inner
fn recurse<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + ?Sized),
    graph: &(impl TreeOps<N> + ?Sized),
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
