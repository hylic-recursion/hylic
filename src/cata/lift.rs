//! Lift infrastructure: `run_lifted` and `run_lifted_zipped`.
//!
//! These free functions execute a lifted computation through any
//! Shared-domain executor. The lift (implementing `LiftOps`) transforms
//! both treeish and fold, the executor runs the result, and `unwrap`
//! extracts the original result type.

use crate::domain::{self, shared};
use crate::graph;
use crate::ops::LiftOps;
use super::exec::Executor;

/// Execute a lifted computation. Shared-domain only.
pub fn run_lifted<N: 'static, R: 'static, N2: 'static, H: Clone + 'static, L: LiftOps<N, R, N2>>(
    exec: &impl Executor<N2, L::LiftedR<H>, domain::Shared, graph::Treeish<N2>>,
    lift: &L,
    fold: &shared::fold::Fold<N, H, R>,
    graph: &graph::Treeish<N>,
    root: &N,
) -> R {
    let lifted_fold = lift.lift_fold(fold.clone());
    let lifted_treeish = lift.lift_treeish(graph.clone());
    let lifted_root = lift.lift_root(root);
    lift.unwrap(exec.run(&lifted_fold, &lifted_treeish, &lifted_root))
}

/// Execute a lifted computation and return both the original and lifted results.
pub fn run_lifted_zipped<N: 'static, R: 'static, N2: 'static, H: Clone + 'static, L: LiftOps<N, R, N2>>(
    exec: &impl Executor<N2, L::LiftedR<H>, domain::Shared, graph::Treeish<N2>>,
    lift: &L,
    fold: &shared::fold::Fold<N, H, R>,
    graph: &graph::Treeish<N>,
    root: &N,
) -> (R, L::LiftedR<H>)
where L::LiftedR<H>: Clone,
{
    let lifted_fold = lift.lift_fold(fold.clone());
    let lifted_treeish = lift.lift_treeish(graph.clone());
    let lifted_root = lift.lift_root(root);
    let inner = exec.run(&lifted_fold, &lifted_treeish, &lifted_root);
    (lift.unwrap(inner.clone()), inner)
}
