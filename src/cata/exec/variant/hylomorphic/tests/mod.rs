//! Hylomorphic executor test suite.
//!
//! One `cargo test hylomorphic` runs everything. Organized by concern:
//!
//! - **correctness**: result matches sequential Fused executor
//! - **stress**: repeated execution under pool churn + lifecycle races
//! - **interleaving**: proves the hylomorphism property (fold during traversal)
//! - **lift_compat**: ParLazy / ParEager lift compatibility

mod correctness;
mod stress;
mod interleaving;
mod lift_compat;

use std::sync::Arc;
use crate::domain::shared as dom;
use crate::prelude::{WorkPool, WorkPoolSpec};
use super::{HylomorphicIn, HylomorphicSpec};

// ── Shared tree types ────────────────────────────────

#[derive(Clone)]
pub(super) struct N {
    pub val: i32,
    pub children: Vec<N>,
}

/// BFS tree builder. big_tree(21, 4) → root with 4 children, each with 4.
pub(super) fn big_tree(n: usize, bf: usize) -> N {
    if n == 0 { return N { val: 0, children: vec![] }; }
    let mut nodes: Vec<N> = (0..n).map(|i| N { val: (i + 1) as i32, children: vec![] }).collect();
    for i in (0..n).rev() {
        let first_child = i * bf + 1;
        if first_child < n {
            let last_child = (first_child + bf).min(n);
            let children: Vec<N> = (first_child..last_child)
                .rev()
                .map(|c| std::mem::replace(&mut nodes[c], N { val: 0, children: vec![] }))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            nodes[i].children = children;
        }
    }
    nodes.into_iter().next().unwrap()
}

/// BFS adjacency list. gen_adj(200, 8) → Arc<Vec<Vec<usize>>> with 200 nodes.
pub(super) fn gen_adj(node_count: usize, bf: usize) -> Arc<Vec<Vec<usize>>> {
    let mut children: Vec<Vec<usize>> = vec![vec![]];
    let mut next_id = 1usize;
    let mut level_start = 0;
    let mut level_end = 1;
    while next_id < node_count {
        let mut new_end = level_end;
        for parent in level_start..level_end {
            let n_ch = bf.min(node_count - next_id);
            if n_ch == 0 { break; }
            let mut my_ch = Vec::with_capacity(n_ch);
            for _ in 0..n_ch {
                if next_id >= node_count { break; }
                children.push(vec![]);
                my_ch.push(next_id);
                next_id += 1;
                new_end += 1;
            }
            children[parent] = my_ch;
        }
        level_start = level_end;
        level_end = new_end;
        if level_start == level_end { break; }
    }
    Arc::new(children)
}

// ── Shared fold/graph constructors ───────────────────

/// Standard sum fold over N trees.
pub(super) fn sum_fold() -> dom::Fold<N, i32, i32> {
    dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; })
}

/// Standard graph for N trees.
pub(super) fn n_graph() -> dom::Treeish<N> {
    dom::treeish(|n: &N| n.children.clone())
}

/// Run a fold with the hylo executor, compare against Fused.
pub(super) fn assert_matches_fused(tree: &N, n_workers: usize) {
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, tree);
    WorkPool::with(WorkPoolSpec::threads(n_workers), |pool| {
        let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(n_workers));
        assert_eq!(exec.run(&fold, &graph, tree), expected);
    });
}
