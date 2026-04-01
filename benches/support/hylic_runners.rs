//! Hylic execution modes — the single source of truth.
//!
//! All 6 modes defined once, generic over node type.
//! `build_all` pre-constructs executor + lift so that
//! only the computation runs inside the benchmark hot loop.

use std::sync::Arc;
use hylic::graph::Treeish;
use hylic::fold::Fold;
use hylic::cata::exec::{self, Executor, ExecutorExt};
use hylic::prelude::{ParLazy, ParEager, WorkPool};

/// A pre-built benchmark mode: name + runner closure.
pub struct BenchMode<'a, R> {
    pub name: &'static str,
    pub run: Box<dyn Fn() -> R + 'a>,
}

/// Build all 6 hylic modes for a given fold + treeish + root.
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
    ]
}

/// Mode names for report generation.
pub const HYLIC_MODES: [&str; 6] = [
    "hylic-fused", "hylic-rayon",
    "hylic-parref+fused", "hylic-parref+rayon",
    "hylic-eager+fused", "hylic-eager+rayon",
];
