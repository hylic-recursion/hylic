//! Hylo-funnel test suite — same rigor as hylo, same test patterns.
//!
//! `make hylic-test-hylo-funnel` runs all funnel tests.

mod correctness;
mod stress;
mod interleaving;

use crate::domain::shared as dom;
use super::{HyloFunnelIn, HyloFunnelSpec};

// ── Shared tree types (same as hylo tests) ───────────

#[derive(Clone)]
pub(super) struct N {
    pub val: i32,
    pub children: Vec<N>,
}

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

pub(super) fn gen_adj(node_count: usize, bf: usize) -> std::sync::Arc<Vec<Vec<usize>>> {
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
    std::sync::Arc::new(children)
}

pub(super) fn sum_fold() -> dom::Fold<N, i32, i32> {
    dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; })
}

pub(super) fn n_graph() -> dom::Treeish<N> {
    dom::treeish(|n: &N| n.children.clone())
}

pub(super) fn assert_matches_fused(tree: &N, n_workers: usize) {
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, tree);
    let exec = HyloFunnelIn::<crate::domain::Shared>::new(n_workers, HyloFunnelSpec::default_for(n_workers));
    assert_eq!(exec.run(&fold, &graph, tree), expected);
}
