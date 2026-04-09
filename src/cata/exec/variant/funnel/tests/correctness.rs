//! Correctness: funnel result matches sequential Fused.
//! Tests cover all named policy presets.

use super::*;

// ── Default (PerWorker + OnFinalize + EveryPush) ─────

#[test]
fn matches_fused_pw() {
    assert_matches_fused_with::<policy::Default>(&big_tree(60, 4), n_threads());
}

#[test]
fn matches_fused_200_pw() {
    assert_matches_fused_with::<policy::Default>(&big_tree(200, 6), n_threads());
}

#[test]
fn zero_workers_pw() {
    assert_matches_fused_with::<policy::Default>(&big_tree(60, 4), 0);
}

// ── SharedDefault (Shared + OnFinalize + EveryPush) ──

#[test]
fn matches_fused_sh() {
    assert_matches_fused_with::<policy::SharedDefault>(&big_tree(60, 4), n_threads());
}

#[test]
fn matches_fused_200_sh() {
    assert_matches_fused_with::<policy::SharedDefault>(&big_tree(200, 6), n_threads());
}

#[test]
fn zero_workers_sh() {
    assert_matches_fused_with::<policy::SharedDefault>(&big_tree(60, 4), 0);
}

// ── WideLight (Shared + OnArrival + EveryPush) ───────

#[test]
fn matches_fused_wl() {
    assert_matches_fused_with::<policy::WideLight>(&big_tree(200, 20), n_threads());
}

// ── LowOverhead (PerWorker + OnFinalize + OncePerBatch)

#[test]
fn matches_fused_lo() {
    assert_matches_fused_with::<policy::LowOverhead>(&big_tree(60, 4), n_threads());
}

// ── PerWorkerArrival (PerWorker + OnArrival + EveryPush)

#[test]
fn matches_fused_pwa() {
    assert_matches_fused_with::<policy::PerWorkerArrival>(&big_tree(200, 6), n_threads());
}

// ── Adjacency list (callback-based treeish) ──────────

fn adjacency_list_noop_impl<P: FunnelPolicy>() {
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
        assert_eq!(exec.run(&fold, &treeish, &0usize), expected);
    });
}

#[test]
fn adjacency_list_noop_pw() { adjacency_list_noop_impl::<policy::Default>(); }

#[test]
fn adjacency_list_noop_sh() { adjacency_list_noop_impl::<policy::SharedDefault>(); }

// ── Wide tree stress (repeated, pool-reused) ─────────

fn wide_tree_stress_impl<P: FunnelPolicy>() {
    let tree = big_tree(200, 20);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    Pool::with(nt, |pool| {
        for i in 0..500 {
            run_on_pool::<P, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
            });
        }
    });
}

#[test]
fn wide_tree_stress_pw() { wide_tree_stress_impl::<policy::Default>(); }

#[test]
fn wide_tree_stress_sh() { wide_tree_stress_impl::<policy::SharedDefault>(); }
