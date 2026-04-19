//! Lift execution: `run_lifted`.
//!
//! Applies a lift's transformation (treeish, fold, root) and runs the
//! result through any Shared-domain executor. Returns `MapR<H, R>`.

use crate::domain::{self, shared};
use crate::graph;
use crate::ops::Lift;
use super::exec::Executor;

/// Execute a lifted computation. Returns the lifted result.
pub fn run_lifted<N: 'static, N2: 'static, H: Clone + 'static, R: Clone + 'static, L: Lift<N, N2>>(
    exec: &impl Executor<N2, L::MapR<H, R>, domain::Shared, graph::Treeish<N2>>,
    lift: &L,
    fold: &shared::fold::Fold<N, H, R>,
    graph: &graph::Treeish<N>,
    root: &N,
) -> L::MapR<H, R> {
    let lifted_fold = lift.lift_fold(fold.clone());
    let lifted_treeish = lift.lift_treeish(graph.clone());
    let lifted_root = lift.lift_root(root);
    exec.run(&lifted_fold, &lifted_treeish, &lifted_root)
}
