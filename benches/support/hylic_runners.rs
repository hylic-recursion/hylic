//! Hylic execution modes — the single source of truth.
//!
//! Each mode is a pre-built closure capturing its executor and optional
//! lift. No string dispatch — construction is typed, only the fold
//! computation runs in the hot loop.

use std::sync::Arc;
use hylic::graph::Treeish;
use hylic::fold::Fold;
use hylic::cata::{Fused, Rayon, Executor};
use hylic::prelude::{ParLazy, ParEager, WorkPool};

/// A pre-built benchmark mode: name + runner closure.
pub struct BenchMode<'a, R> {
    pub name: &'static str,
    pub run: Box<dyn Fn() -> R + 'a>,
}

/// Build all 6 hylic modes for a given fold + treeish + root.
/// Executors and lifts are constructed here — only the fold
/// computation runs when `mode.run()` is called.
pub fn build_all<'a, N, H, R>(
    fold: &'a Fold<N, H, R>,
    treeish: &'a Treeish<N>,
    root: &'a N,
    pool: &'a Arc<WorkPool>,
) -> Vec<BenchMode<'a, R>>
where
    N: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    let par_lazy = ParLazy::lift::<N, H, R>();
    let par_lazy2 = par_lazy.clone();
    let par_eager_fused = ParEager::lift::<N, H, R>(pool);
    let par_eager_rayon = ParEager::lift::<N, H, R>(pool);

    vec![
        BenchMode { name: "hylic-fused",
            run: Box::new(move || Fused.run(fold, treeish, root)) },
        BenchMode { name: "hylic-rayon",
            run: Box::new(move || Rayon.run(fold, treeish, root)) },
        BenchMode { name: "hylic-parref+fused",
            run: Box::new(move || Fused.run_lifted(&par_lazy, fold, treeish, root)) },
        BenchMode { name: "hylic-parref+rayon",
            run: Box::new(move || Rayon.run_lifted(&par_lazy2, fold, treeish, root)) },
        BenchMode { name: "hylic-eager+fused",
            run: Box::new(move || Fused.run_lifted(&par_eager_fused, fold, treeish, root)) },
        BenchMode { name: "hylic-eager+rayon",
            run: Box::new(move || Rayon.run_lifted(&par_eager_rayon, fold, treeish, root)) },
    ]
}

/// Mode names for report generation (must match build_all order).
pub const HYLIC_MODES: [&str; 6] = [
    "hylic-fused",
    "hylic-rayon",
    "hylic-parref+fused",
    "hylic-parref+rayon",
    "hylic-eager+fused",
    "hylic-eager+rayon",
];
