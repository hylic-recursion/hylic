//! Correctness: hylo result matches sequential Fused on various tree shapes.

use super::*;

/// 60 nodes, bf=4 (3 levels), all available threads.
#[test]
fn matches_fused() {
    assert_matches_fused(&big_tree(60, 4), n_threads());
}

/// 200 nodes, bf=6 (3 levels), all available threads.
#[test]
fn matches_fused_200() {
    assert_matches_fused(&big_tree(200, 6), n_threads());
}

/// All work done by the calling thread (no worker threads).
/// Guards: help_once loop processes the entire tree solo.
#[test]
fn zero_workers() {
    assert_matches_fused(&big_tree(60, 4), 0);
}

/// Adjacency-list graph with NodeId=usize, noop fold.
/// Guards: the treeish_visit callback path (not treeish clone path).
#[test]
fn adjacency_list_noop() {
    let adj = gen_adj(200, 8);
    let ch = adj.clone();
    let treeish = dom::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &child in &ch[*n] { cb(&child); }
    });
    let fold = dom::fold(
        |_: &usize| 0u64,
        |h: &mut u64, c: &u64| { *h += c; },
        |h: &u64| *h,
    );
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);
    let nt = n_threads();
    WorkPool::with(WorkPoolSpec::threads(nt), |pool| {
        let exec = HylomorphicIn::<crate::domain::Shared>::new(
            pool, HylomorphicSpec::default_for(nt));
        assert_eq!(exec.run(&fold, &treeish, &0usize), expected);
    });
}
