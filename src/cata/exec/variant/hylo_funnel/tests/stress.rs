//! Stress tests: repeated execution, lifecycle churn.
//! High iteration counts to catch timing-sensitive races.
//! Each test runs for both PerWorker and Shared queue strategies.

use super::*;
use super::super::queue;

// ── stress_1500_runs ─────────────────────────────────

fn stress_1500_runs_impl<W: WorkStealing>() {
    let tree = big_tree(200, 6);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    for i in 0..1500 {
        let exec = make_exec::<W>(nt);
        assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
    }
}

#[test]
fn stress_1500_runs_pw() { stress_1500_runs_impl::<queue::PerWorker>(); }

#[test]
fn stress_1500_runs_sh() { stress_1500_runs_impl::<queue::Shared>(); }

// ── stress_1500_runs_adjacency ───────────────────────

fn stress_1500_runs_adjacency_impl<W: WorkStealing>() {
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
    let exec = make_exec::<W>(n_threads());
    for i in 0..1500 {
        assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "iteration {i}");
    }
}

#[test]
fn stress_1500_runs_adjacency_pw() { stress_1500_runs_adjacency_impl::<queue::PerWorker>(); }

#[test]
fn stress_1500_runs_adjacency_sh() { stress_1500_runs_adjacency_impl::<queue::Shared>(); }

// ── pool_lifecycle_1500 ──────────────────────────────

fn pool_lifecycle_impl<W: WorkStealing>() {
    let tree = big_tree(10, 3);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    for _ in 0..5000 {
        let exec = make_exec::<W>(nt);
        assert_eq!(exec.run(&fold, &graph, &tree), expected);
    }
}

#[test]
fn pool_lifecycle_pw() { pool_lifecycle_impl::<queue::PerWorker>(); }

#[test]
fn pool_lifecycle_sh() { pool_lifecycle_impl::<queue::Shared>(); }

// ── wide tree with reused FoldContext (matches benchmark pattern) ──

fn wide_foldctx_reuse_impl<W: WorkStealing>() {
    use super::super::run::{FoldContext, run_fold_with};
    use super::super::pool::FunnelPool;
    use super::super::AccumulateMode;
    let adj = gen_adj(200, 20);
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
    let pool = FunnelPool::new(nt);
    let mut fctx = FoldContext::<usize, u64, u64, W>::new(&Default::default(), nt);
    for i in 0..3000 {
        fctx.reset();
        let result = run_fold_with::<_, _, _, _, _, W>(
            &fold, &treeish, &0usize, &pool, AccumulateMode::OnArrival, &mut fctx,
        );
        assert_eq!(result, expected, "iteration {i}");
    }
}

#[test]
fn wide_foldctx_reuse_pw() { wide_foldctx_reuse_impl::<queue::PerWorker>(); }

#[test]
fn wide_foldctx_reuse_sh() { wide_foldctx_reuse_impl::<queue::Shared>(); }
