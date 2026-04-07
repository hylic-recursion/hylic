//! Stress tests: repeated execution, lifecycle churn.
//! High iteration counts to catch timing-sensitive races.
//! Each test runs for both Default and SharedDefault policies.

use super::*;

fn stress_1500_runs_impl<P: FunnelPolicy>() {
    let tree = big_tree(200, 6);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    for i in 0..1500 {
        with_exec::<P, _>(nt, |exec| {
            assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
        });
    }
}

#[test]
fn stress_1500_runs_pw() { stress_1500_runs_impl::<policy::Default>(); }

#[test]
fn stress_1500_runs_sh() { stress_1500_runs_impl::<policy::SharedDefault>(); }

fn stress_1500_runs_adjacency_impl<P: FunnelPolicy>() {
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
    with_exec::<P, _>(n_threads(), |exec| {
        for i in 0..1500 {
            assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "iteration {i}");
        }
    });
}

#[test]
fn stress_1500_runs_adjacency_pw() { stress_1500_runs_adjacency_impl::<policy::Default>(); }

#[test]
fn stress_1500_runs_adjacency_sh() { stress_1500_runs_adjacency_impl::<policy::SharedDefault>(); }

fn pool_lifecycle_impl<P: FunnelPolicy>() {
    let tree = big_tree(10, 3);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    for _ in 0..5000 {
        with_exec::<P, _>(nt, |exec| {
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }
}

#[test]
fn pool_lifecycle_pw() { pool_lifecycle_impl::<policy::Default>(); }

#[test]
fn pool_lifecycle_sh() { pool_lifecycle_impl::<policy::SharedDefault>(); }

