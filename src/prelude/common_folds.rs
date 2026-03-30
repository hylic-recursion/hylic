//! Common fold patterns — ready-made folds for frequent use cases.

use crate::fold::{Fold, simple_fold};
use crate::graph::Treeish;
use crate::cata::Strategy;

/// Count all nodes in a tree.
pub fn count_fold<N: 'static>() -> Fold<N, usize, usize> {
    simple_fold(|_: &N| 1usize, |heap: &mut usize, child: &usize| *heap += child)
}

/// Maximum depth of a tree (root = 1).
pub fn depth_fold<N: 'static>() -> Fold<N, usize, usize> {
    simple_fold(
        |_: &N| 1usize,
        |heap: &mut usize, child: &usize| *heap = (*heap).max(*child + 1),
    )
}

/// Format a tree as an indented string and return it.
pub fn pretty_print<N: Send + Sync + 'static>(
    strategy: Strategy,
    graph: &Treeish<N>,
    root: &N,
    format_node: impl Fn(&N) -> String + Send + Sync + 'static,
) -> String
where N: Clone
{
    use crate::prelude::format::TreeFormatCfg;
    let cfg = TreeFormatCfg::default_multiline(format_node);
    let fold = cfg.make_fold();
    strategy.run(&fold, graph, root)
}
