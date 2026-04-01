//! Hylic execution modes — the single source of truth.
//!
//! All modes defined once. `build_all` pre-constructs executor + lift
//! so that only the computation runs inside the benchmark hot loop.
//! Domain variants (Local, Owned) are included to measure boxing overhead.

use std::sync::Arc;
use hylic::cata::exec::{self, Executor, ExecutorExt};
use hylic::prelude::{ParLazy, ParEager, WorkPool};

use super::tree::NodeId;
use super::scenario::PreparedScenario;

/// A pre-built benchmark mode: name + runner closure.
pub struct BenchMode<'a, R> {
    pub name: &'static str,
    pub run: Box<dyn Fn() -> R + 'a>,
}

/// Build all hylic modes for a PreparedScenario.
/// Includes Shared (6 modes), Local (1 mode), Owned (1 mode).
pub fn build_all<'a>(
    s: &'a PreparedScenario,
    pool: &'a Arc<WorkPool>,
) -> Vec<BenchMode<'a, u64>> {
    let fold = &s.fold;
    let treeish = &s.treeish;
    let root = &s.root;

    // Pre-build Lifts outside the hot loop
    let par_lazy = ParLazy::lift::<NodeId, u64, u64>();
    let par_lazy2 = par_lazy.clone();
    let par_eager_fused = ParEager::lift::<NodeId, u64, u64>(pool);
    let par_eager_rayon = ParEager::lift::<NodeId, u64, u64>(pool);

    // Local domain: construct once, reuse across iterations
    let local_fold = {
        let i = s.init_fn.clone();
        let a = s.acc_fn.clone();
        let f = s.fin_fn.clone();
        hylic::domain::local::fold(move |n: &NodeId| i(n), move |h: &mut u64, c: &u64| a(h, c), move |h: &u64| f(h))
    };
    let local_treeish = {
        let g = s.graph_fn.clone();
        hylic::domain::local::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| g(n, cb))
    };

    // Owned domain: must reconstruct each call (not Clone)
    let owned_init = s.init_fn.clone();
    let owned_acc = s.acc_fn.clone();
    let owned_fin = s.fin_fn.clone();
    let owned_graph = s.graph_fn.clone();

    vec![
        // ── Shared domain ──────────────────────────────
        BenchMode { name: "hylic-fused",
            run: Box::new(move || exec::FUSED.run(fold, treeish, root)) },
        BenchMode { name: "hylic-rayon",
            run: Box::new(move || exec::RAYON.run(fold, treeish, root)) },
        BenchMode { name: "hylic-parref+fused",
            run: Box::new(move || exec::FUSED.run_lifted(&par_lazy, fold, treeish, root)) },
        BenchMode { name: "hylic-parref+rayon",
            run: Box::new(move || exec::RAYON.run_lifted(&par_lazy2, fold, treeish, root)) },
        BenchMode { name: "hylic-eager+fused",
            run: Box::new(move || exec::FUSED.run_lifted(&par_eager_fused, fold, treeish, root)) },
        BenchMode { name: "hylic-eager+rayon",
            run: Box::new(move || exec::RAYON.run_lifted(&par_eager_rayon, fold, treeish, root)) },

        // ── Local domain (Rc, no atomics) ──────────────
        BenchMode { name: "hylic-fused-local",
            run: Box::new(move || exec::FUSED_LOCAL.run(&local_fold, &local_treeish, root)) },

        // ── Owned domain (Box, zero refcount) ──────────
        BenchMode { name: "hylic-fused-owned",
            run: Box::new(move || {
                let i = owned_init.clone();
                let a = owned_acc.clone();
                let f = owned_fin.clone();
                let g = owned_graph.clone();
                let fold = hylic::domain::owned::fold(
                    move |n: &NodeId| i(n),
                    move |h: &mut u64, c: &u64| a(h, c),
                    move |h: &u64| f(h),
                );
                let graph = hylic::domain::owned::treeish_visit(
                    move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| g(n, cb),
                );
                exec::FUSED_OWNED.run(&fold, &graph, root)
            }) },
    ]
}

/// Mode names for report generation.
pub const HYLIC_MODES: [&str; 8] = [
    "hylic-fused", "hylic-rayon",
    "hylic-parref+fused", "hylic-parref+rayon",
    "hylic-eager+fused", "hylic-eager+rayon",
    "hylic-fused-local", "hylic-fused-owned",
];
