//! Stress tests: repeated execution, lifecycle churn.

use super::*;

#[test]
fn stress_200_runs() {
    let tree = big_tree(200, 6);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    for i in 0..200 {
        let exec = HyloFunnelIn::<crate::domain::Shared>::new(4, HyloFunnelSpec::default_for(4));
        assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
    }
}

#[test]
fn stress_200_runs_adjacency() {
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
    let exec = HyloFunnelIn::<crate::domain::Shared>::new(3, HyloFunnelSpec::default_for(3));
    for i in 0..200 {
        assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "iteration {i}");
    }
}

/// Pool lifecycle under rapid create/destroy — exercises eventcount
/// shutdown + thread join. Each exec.run creates and destroys a pool.
#[test]
fn pool_lifecycle_500() {
    let tree = big_tree(10, 3);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    for _ in 0..500 {
        let exec = HyloFunnelIn::<crate::domain::Shared>::new(4, HyloFunnelSpec::default_for(4));
        assert_eq!(exec.run(&fold, &graph, &tree), expected);
    }
}
