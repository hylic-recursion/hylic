//! Common fold patterns — ready-made folds for frequent use cases.

use crate::domain::shared::fold::{Fold, simple_fold};
use crate::graph::Treeish;
use crate::cata::exec;
use crate::domain;
use crate::prelude::vec_fold::{vec_fold, VecHeap};
use crate::prelude::utils::push_indent;

/// Count all nodes in a tree.
pub fn count_fold<N: 'static>() -> Fold<N, usize, usize> {
    let init = |_: &N| 1usize;
    let acc = |heap: &mut usize, child: &usize| *heap += child;
    simple_fold(init, acc)
}

/// Maximum depth of a tree (root = 1).
pub fn depth_fold<N: 'static>() -> Fold<N, usize, usize> {
    let init = |_: &N| 1usize;
    let acc = |heap: &mut usize, child: &usize| *heap = (*heap).max(*child + 1);
    simple_fold(init, acc)
}

/// Format a tree as an indented string.
pub fn pretty_print<N: Clone + 'static>(
    exec: &impl exec::Executor<N, String, domain::Shared>,
    graph: &Treeish<N>,
    root: &N,
    format_node: impl Fn(&N) -> String + Send + Sync + 'static,
) -> String {
    let format = move |heap: &VecHeap<N, String>| {
        let label = format_node(&heap.node);
        if heap.childresults.is_empty() { return label; }
        let children = heap.childresults.join(",\n");
        format!("{}[\n{}\n]", label, push_indent(&children, "  "))
    };
    let fold = vec_fold(format);
    exec.run(&fold, graph, root)
}
