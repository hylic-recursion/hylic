//! Domain tests: verify that all domains produce identical results
//! with domain-compatible executors.

use hylic::cata::exec::{self, Executor};

// ── Shared tree fixture ───────────────────────────

#[derive(Clone)]
struct N { val: i32, children: Vec<N> }

fn sample_tree() -> N {
    N { val: 1, children: vec![
        N { val: 2, children: vec![N { val: 4, children: vec![] }] },
        N { val: 3, children: vec![] },
    ]}
}

const EXPECTED: u64 = 10; // 1 + 2 + 4 + 3

// Closures defined ONCE — reused across all domains.
fn sum_init(n: &N) -> u64 { n.val as u64 }
fn sum_acc(heap: &mut u64, child: &u64) { *heap += child; }
fn sum_fin(heap: &u64) -> u64 { *heap }

fn tree_children(n: &N, cb: &mut dyn FnMut(&N)) {
    for child in &n.children { cb(child); }
}

// ── Shared domain ─────────────────────────────────

#[test]
fn shared_fused() {
    let fold = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let graph = hylic::graph::treeish_visit(tree_children);
    assert_eq!(exec::FUSED.run(&fold, &graph, &sample_tree()), EXPECTED);
}

#[test]
fn shared_sequential() {
    let fold = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let graph = hylic::graph::treeish_visit(tree_children);
    assert_eq!(exec::SEQUENTIAL.run(&fold, &graph, &sample_tree()), EXPECTED);
}

#[test]
fn shared_rayon() {
    let fold = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let graph = hylic::graph::treeish_visit(tree_children);
    assert_eq!(exec::RAYON.run(&fold, &graph, &sample_tree()), EXPECTED);
}

// ── Local domain ──────────────────────────────────

#[test]
fn local_fused() {
    let fold = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let graph = hylic::domain::local::treeish_visit(tree_children);
    assert_eq!(exec::FUSED_LOCAL.run(&fold, &graph, &sample_tree()), EXPECTED);
}

#[test]
fn local_sequential() {
    let fold = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let graph = hylic::domain::local::treeish_visit(tree_children);
    assert_eq!(exec::SEQUENTIAL_LOCAL.run(&fold, &graph, &sample_tree()), EXPECTED);
}

// ── Owned domain ──────────────────────────────────

#[test]
fn owned_fused() {
    let fold = hylic::domain::owned::fold(sum_init, sum_acc, sum_fin);
    let graph = hylic::domain::owned::treeish_visit(tree_children);
    assert_eq!(exec::FUSED_OWNED.run(&fold, &graph, &sample_tree()), EXPECTED);
}

#[test]
fn owned_sequential() {
    let fold = hylic::domain::owned::fold(sum_init, sum_acc, sum_fin);
    let graph = hylic::domain::owned::treeish_visit(tree_children);
    assert_eq!(exec::SEQUENTIAL_OWNED.run(&fold, &graph, &sample_tree()), EXPECTED);
}

// ── Cross-domain agreement ────────────────────────

#[test]
fn all_domains_agree() {
    let tree = sample_tree();

    // Shared
    let sf = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let sg = hylic::graph::treeish_visit(tree_children);
    let shared_result = exec::FUSED.run(&sf, &sg, &tree);

    // Local
    let lf = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let lg = hylic::domain::local::treeish_visit(tree_children);
    let local_result = exec::FUSED_LOCAL.run(&lf, &lg, &tree);

    // Owned
    let of = hylic::domain::owned::fold(sum_init, sum_acc, sum_fin);
    let og = hylic::domain::owned::treeish_visit(tree_children);
    let owned_result = exec::FUSED_OWNED.run(&of, &og, &tree);

    assert_eq!(shared_result, EXPECTED);
    assert_eq!(local_result, EXPECTED);
    assert_eq!(owned_result, EXPECTED);
}
