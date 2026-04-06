//! Correctness: funnel result matches sequential Fused.
//! Each test runs for both PerWorker and Shared queue strategies.

use super::*;
use super::super::queue;

// ── matches_fused ─────────────────��───────────────��──

#[test]
fn matches_fused_pw() {
    assert_matches_fused_with::<queue::PerWorker>(&big_tree(60, 4), n_threads());
}

#[test]
fn matches_fused_sh() {
    assert_matches_fused_with::<queue::Shared>(&big_tree(60, 4), n_threads());
}

// ── matches_fused_200 ───────────────────���────────────

#[test]
fn matches_fused_200_pw() {
    assert_matches_fused_with::<queue::PerWorker>(&big_tree(200, 6), n_threads());
}

#[test]
fn matches_fused_200_sh() {
    assert_matches_fused_with::<queue::Shared>(&big_tree(200, 6), n_threads());
}

// ── zero_workers ───────────────────────────��─────────

#[test]
fn zero_workers_pw() {
    assert_matches_fused_with::<queue::PerWorker>(&big_tree(60, 4), 0);
}

#[test]
fn zero_workers_sh() {
    assert_matches_fused_with::<queue::Shared>(&big_tree(60, 4), 0);
}

// ── adjacency_list_noop ────────────────────���─────────

fn adjacency_list_noop_impl<W: WorkStealing>() {
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
    assert_eq!(exec.run(&fold, &treeish, &0usize), expected);
}

#[test]
fn adjacency_list_noop_pw() { adjacency_list_noop_impl::<queue::PerWorker>(); }

#[test]
fn adjacency_list_noop_sh() { adjacency_list_noop_impl::<queue::Shared>(); }

// ── wide tree (bf=20, triggers overflow in FoldChain) ──

fn wide_tree_impl<W: WorkStealing>() {
    assert_matches_fused_with::<W>(&big_tree(200, 20), n_threads());
}

#[test]
fn wide_tree_pw() { wide_tree_impl::<queue::PerWorker>(); }

#[test]
fn wide_tree_sh() { wide_tree_impl::<queue::Shared>(); }

// ── wide tree stress (catches overflow race under contention) ──

fn wide_tree_stress_impl<W: WorkStealing>() {
    let tree = big_tree(200, 20);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let exec = make_exec::<W>(n_threads());
    for i in 0..500 {
        assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
    }
}

#[test]
fn wide_tree_stress_pw() { wide_tree_stress_impl::<queue::PerWorker>(); }

#[test]
fn wide_tree_stress_sh() { wide_tree_stress_impl::<queue::Shared>(); }
