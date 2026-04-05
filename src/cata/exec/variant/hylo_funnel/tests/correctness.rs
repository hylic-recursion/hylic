//! Correctness: funnel result matches sequential Fused.

use super::*;

#[test]
fn matches_fused() {
    assert_matches_fused(&big_tree(60, 4), n_threads());
}

#[test]
fn matches_fused_200() {
    assert_matches_fused(&big_tree(200, 6), n_threads());
}

#[test]
fn zero_workers() {
    assert_matches_fused(&big_tree(60, 4), 0);
}

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
    let exec = HyloFunnelIn::<crate::domain::Shared>::new(nt, HyloFunnelSpec::default_for(nt));
    assert_eq!(exec.run(&fold, &treeish, &0usize), expected);
}
