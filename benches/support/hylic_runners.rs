//! Hylic execution modes — the single source of truth.
//!
//! All 6 modes defined once, generic over node type.
//! Every benchmark calls `run_hylic_mode` with a fold + treeish.

use std::sync::Arc;
use hylic::graph::Treeish;
use hylic::fold::Fold;
use hylic::cata::Exec;
use hylic::prelude::{ParLazy, ParEager, WorkPool};

/// The 6 hylic execution modes: 3 execs × {none, ParLazy, ParEager}.
pub const HYLIC_MODES: [&str; 6] = [
    "hylic-fused",
    "hylic-rayon",
    "hylic-parref+fused",
    "hylic-parref+rayon",
    "hylic-eager+fused",
    "hylic-eager+rayon",
];

/// Run one hylic mode. Generic over node type N.
pub fn run_hylic_mode<N, H, R>(
    mode: &str,
    fold: &Fold<N, H, R>,
    treeish: &Treeish<N>,
    root: &N,
    pool: &Arc<WorkPool>,
) -> R
where
    N: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    match mode {
        "hylic-fused"        => Exec::fused().run(fold, treeish, root),
        "hylic-rayon"        => Exec::rayon().run(fold, treeish, root),
        "hylic-parref+fused" => Exec::fused().run_lifted(&ParLazy::lift(), fold, treeish, root),
        "hylic-parref+rayon" => Exec::rayon().run_lifted(&ParLazy::lift(), fold, treeish, root),
        "hylic-eager+fused"  => Exec::fused().run_lifted(&ParEager::lift(pool), fold, treeish, root),
        "hylic-eager+rayon"  => Exec::rayon().run_lifted(&ParEager::lift(pool), fold, treeish, root),
        _ => panic!("unknown hylic mode: {mode}"),
    }
}
