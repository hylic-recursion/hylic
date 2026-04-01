//! Domain tests: verify that all domains produce identical results
//! with domain-compatible executors, and that transformations work
//! across clone-able domains (Shared, Local).

use hylic::cata::exec::{self, Executor};

// ── Tree fixture ──────────────────────────────────

#[derive(Clone)]
struct N { val: i32, children: Vec<N> }

fn sample_tree() -> N {
    N { val: 1, children: vec![
        N { val: 2, children: vec![N { val: 4, children: vec![] }] },
        N { val: 3, children: vec![] },
    ]}
}

const EXPECTED: u64 = 10; // 1 + 2 + 4 + 3

fn sum_init(n: &N) -> u64 { n.val as u64 }
fn sum_acc(heap: &mut u64, child: &u64) { *heap += child; }
fn sum_fin(heap: &u64) -> u64 { *heap }

fn tree_children(n: &N, cb: &mut dyn FnMut(&N)) {
    for child in &n.children { cb(child); }
}

// ── Shared domain: executors ──────────────────────

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

// ── Local domain: executors ───────────────────────

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

// ── Owned domain: executors ───────────────────────

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

    let sf = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let sg = hylic::graph::treeish_visit(tree_children);
    let shared_result = exec::FUSED.run(&sf, &sg, &tree);

    let lf = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let lg = hylic::domain::local::treeish_visit(tree_children);
    let local_result = exec::FUSED_LOCAL.run(&lf, &lg, &tree);

    let of = hylic::domain::owned::fold(sum_init, sum_acc, sum_fin);
    let og = hylic::domain::owned::treeish_visit(tree_children);
    let owned_result = exec::FUSED_OWNED.run(&of, &og, &tree);

    assert_eq!(shared_result, EXPECTED);
    assert_eq!(local_result, EXPECTED);
    assert_eq!(owned_result, EXPECTED);
}

// ── Shared transformations ────────────────────────

#[test]
fn shared_map() {
    let fold = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let mapped = fold.map(
        |r: &u64| format!("v={}", r),
        |s: &String| s.strip_prefix("v=").unwrap().parse().unwrap(),
    );
    let graph = hylic::graph::treeish_visit(tree_children);
    let result = exec::FUSED.run(&mapped, &graph, &sample_tree());
    assert_eq!(result, "v=10");
}

#[test]
fn shared_zipmap() {
    let fold = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let zipped = fold.zipmap(|r: &u64| *r > 5);
    let graph = hylic::graph::treeish_visit(tree_children);
    let (sum, over_five) = exec::FUSED.run(&zipped, &graph, &sample_tree());
    assert_eq!(sum, EXPECTED);
    assert!(over_five);
}

#[test]
fn shared_contramap() {
    // Contramap: change node type from String → N
    let fold = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let contramapped = fold.contramap(|s: &String| N { val: s.len() as i32, children: vec![] });
    let graph = hylic::graph::treeish_visit(|_: &String, _cb: &mut dyn FnMut(&String)| {});
    let result = exec::FUSED.run(&contramapped, &graph, &"hello".to_string());
    assert_eq!(result, 5);
}

#[test]
fn shared_product() {
    let sum_fold = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let count_fold = hylic::fold::fold(
        |_: &N| 1u32,
        |h: &mut u32, c: &u32| *h += c,
        |h: &u32| *h,
    );
    let combined = sum_fold.product(&count_fold);
    let graph = hylic::graph::treeish_visit(tree_children);
    let (sum, count) = exec::FUSED.run(&combined, &graph, &sample_tree());
    assert_eq!(sum, EXPECTED);
    assert_eq!(count, 4); // 4 nodes
}

// ── Local transformations ─────────────────────────

#[test]
fn local_map() {
    let fold = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let mapped = fold.map(
        |r: &u64| format!("v={}", r),
        |s: &String| s.strip_prefix("v=").unwrap().parse().unwrap(),
    );
    let graph = hylic::domain::local::treeish_visit(tree_children);
    let result = exec::FUSED_LOCAL.run(&mapped, &graph, &sample_tree());
    assert_eq!(result, "v=10");
}

#[test]
fn local_zipmap() {
    let fold = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let zipped = fold.zipmap(|r: &u64| *r > 5);
    let graph = hylic::domain::local::treeish_visit(tree_children);
    let (sum, over_five) = exec::FUSED_LOCAL.run(&zipped, &graph, &sample_tree());
    assert_eq!(sum, EXPECTED);
    assert!(over_five);
}

#[test]
fn local_contramap() {
    let fold = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let contramapped = fold.contramap(|s: &String| N { val: s.len() as i32, children: vec![] });
    let graph = hylic::domain::local::treeish_visit(|_: &String, _cb: &mut dyn FnMut(&String)| {});
    let result = exec::FUSED_LOCAL.run(&contramapped, &graph, &"hello".to_string());
    assert_eq!(result, 5);
}

#[test]
fn local_product() {
    let sum_fold = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let count_fold = hylic::domain::local::fold(
        |_: &N| 1u32,
        |h: &mut u32, c: &u32| *h += c,
        |h: &u32| *h,
    );
    let combined = sum_fold.product(&count_fold);
    let graph = hylic::domain::local::treeish_visit(tree_children);
    let (sum, count) = exec::FUSED_LOCAL.run(&combined, &graph, &sample_tree());
    assert_eq!(sum, EXPECTED);
    assert_eq!(count, 4);
}

// ── Local + Shared transformations agree ──────────

#[test]
fn transformations_agree_across_domains() {
    let tree = sample_tree();
    let sg = hylic::graph::treeish_visit(tree_children);
    let lg = hylic::domain::local::treeish_visit(tree_children);

    // map
    let sf = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let lf = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let sm = sf.map(|r: &u64| *r * 2, |r: &u64| *r / 2);
    let lm = lf.map(|r: &u64| *r * 2, |r: &u64| *r / 2);
    assert_eq!(
        exec::FUSED.run(&sm, &sg, &tree),
        exec::FUSED_LOCAL.run(&lm, &lg, &tree),
    );

    // zipmap
    let sf = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let lf = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let sz = sf.zipmap(|r: &u64| *r > 5);
    let lz = lf.zipmap(|r: &u64| *r > 5);
    assert_eq!(
        exec::FUSED.run(&sz, &sg, &tree),
        exec::FUSED_LOCAL.run(&lz, &lg, &tree),
    );

    // product
    let sf = hylic::fold::fold(sum_init, sum_acc, sum_fin);
    let lf = hylic::domain::local::fold(sum_init, sum_acc, sum_fin);
    let sc = hylic::fold::fold(|_: &N| 1u32, |h: &mut u32, c: &u32| *h += c, |h: &u32| *h);
    let lc = hylic::domain::local::fold(|_: &N| 1u32, |h: &mut u32, c: &u32| *h += c, |h: &u32| *h);
    let sp = sf.product(&sc);
    let lp = lf.product(&lc);
    assert_eq!(
        exec::FUSED.run(&sp, &sg, &tree),
        exec::FUSED_LOCAL.run(&lp, &lg, &tree),
    );
}

// ── Owned: simple_fold works ──────────────────────

#[test]
fn owned_simple_fold() {
    let fold = hylic::domain::owned::simple_fold(sum_init, sum_acc);
    let graph = hylic::domain::owned::treeish_visit(tree_children);
    assert_eq!(exec::FUSED_OWNED.run(&fold, &graph, &sample_tree()), EXPECTED);
}

// ── Local: simple_fold works ──────────────────────

#[test]
fn local_simple_fold() {
    let fold = hylic::domain::local::simple_fold(sum_init, sum_acc);
    let graph = hylic::domain::local::treeish_visit(tree_children);
    assert_eq!(exec::FUSED_LOCAL.run(&fold, &graph, &sample_tree()), EXPECTED);
}
