//! Hylic execution modes — the single source of truth.
//!
//! All modes defined once. `build_all` pre-constructs executor + lift
//! so that only the computation runs inside the benchmark hot loop.
//! Domain variants construct from WorkSpec directly — same single
//! layer of indirection as Shared. No double wrapping.

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

    // Local domain: single layer of Rc, from WorkSpec directly
    let local_fold = {
        let w1 = s.work.clone();
        let w2 = s.work.clone();
        let w3 = s.work.clone();
        hylic::domain::local::fold(
            move |_: &NodeId| w1.do_init(),
            move |h: &mut u64, c: &u64| w2.do_accumulate(h, c),
            move |h: &u64| w3.do_finalize(h),
        )
    };
    let local_treeish = {
        let w = s.work.clone();
        let ch = s.children.clone();
        hylic::domain::local::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
            w.do_graph();
            for &child in &ch[*n] { cb(&child); }
        })
    };

    // Owned domain: single layer of Box, reconstructed per call (not Clone)
    let owned_work = s.work.clone();
    let owned_children = s.children.clone();

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

        // ── Local domain (Rc, single layer) ────────────
        BenchMode { name: "hylic-fused-local",
            run: Box::new(move || exec::FUSED_LOCAL.run(&local_fold, &local_treeish, root)) },

        // ── Owned domain (Box, single layer, reconstructed per call) ──
        BenchMode { name: "hylic-fused-owned",
            run: Box::new(move || {
                let w1 = owned_work.clone();
                let w2 = owned_work.clone();
                let w3 = owned_work.clone();
                let wg = owned_work.clone();
                let ch = owned_children.clone();
                let fold = hylic::domain::owned::fold(
                    move |_: &NodeId| w1.do_init(),
                    move |h: &mut u64, c: &u64| w2.do_accumulate(h, c),
                    move |h: &u64| w3.do_finalize(h),
                );
                let graph = hylic::domain::owned::treeish_visit(
                    move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
                        wg.do_graph();
                        for &child in &ch[*n] { cb(&child); }
                    },
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
