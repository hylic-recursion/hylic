//! Lift: paired transformation of Treeish + Fold.
//!
//! Operates on Shared-domain types (Arc-based). Lifts transform a
//! computation to a different type domain — the fold sees different
//! heap/result types while the graph sees different node types.
//!
//! `run_lifted` and `run_lifted_zipped` execute a lifted computation
//! through any Shared-domain executor.

use crate::domain::{self, shared};
use crate::graph;
use crate::ops::LiftOps;
use super::exec::Executor;

/// A paired transformation that lifts Treeish + Fold to a different
/// type domain. Purely a transformation — knows nothing about execution.
// ANCHOR: lift_struct
pub struct Lift<N, H, R, N2, H2, R2>
where
    N: 'static, H: 'static, R: 'static,
    N2: 'static, H2: 'static, R2: 'static,
{
    pub(crate) impl_lift_treeish: Box<dyn Fn(graph::Treeish<N>) -> graph::Treeish<N2>>,
    pub(crate) impl_lift_fold: Box<dyn Fn(shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, H2, R2>>,
    pub(crate) impl_lift_root: Box<dyn Fn(&N) -> N2>,
    pub(crate) impl_unwrap: Box<dyn Fn(R2) -> R>,
}
// ANCHOR_END: lift_struct

impl<N, H, R, N2, H2, R2> Lift<N, H, R, N2, H2, R2>
where
    N: 'static, H: 'static, R: 'static,
    N2: 'static, H2: 'static, R2: 'static,
{
    pub fn new(
        lift_treeish: impl Fn(graph::Treeish<N>) -> graph::Treeish<N2> + 'static,
        lift_fold: impl Fn(shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, H2, R2> + 'static,
        lift_root: impl Fn(&N) -> N2 + 'static,
        unwrap: impl Fn(R2) -> R + 'static,
    ) -> Self {
        Lift {
            impl_lift_treeish: Box::new(lift_treeish),
            impl_lift_fold: Box::new(lift_fold),
            impl_lift_root: Box::new(lift_root),
            impl_unwrap: Box::new(unwrap),
        }
    }

    pub fn map_lifted_fold(self, mapper: impl Fn(shared::fold::Fold<N2, H2, R2>) -> shared::fold::Fold<N2, H2, R2> + 'static) -> Self {
        let orig = self.impl_lift_fold;
        Lift {
            impl_lift_treeish: self.impl_lift_treeish,
            impl_lift_fold: Box::new(move |fold| mapper(orig(fold))),
            impl_lift_root: self.impl_lift_root,
            impl_unwrap: self.impl_unwrap,
        }
    }

    pub fn map_lifted_treeish(self, mapper: impl Fn(graph::Treeish<N2>) -> graph::Treeish<N2> + 'static) -> Self {
        let orig = self.impl_lift_treeish;
        Lift {
            impl_lift_treeish: Box::new(move |treeish| mapper(orig(treeish))),
            impl_lift_fold: self.impl_lift_fold,
            impl_lift_root: self.impl_lift_root,
            impl_unwrap: self.impl_unwrap,
        }
    }
}

impl<N, H, R, N2, H2, R2> LiftOps<N, H, R, N2, H2, R2>
    for Lift<N, H, R, N2, H2, R2>
where
    N: 'static, H: 'static, R: 'static,
    N2: 'static, H2: 'static, R2: 'static,
{
    fn lift_treeish(&self, t: graph::Treeish<N>) -> graph::Treeish<N2> {
        (self.impl_lift_treeish)(t)
    }
    fn lift_fold(&self, f: shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, H2, R2> {
        (self.impl_lift_fold)(f)
    }
    fn lift_root(&self, root: &N) -> N2 {
        (self.impl_lift_root)(root)
    }
    fn unwrap(&self, result: R2) -> R {
        (self.impl_unwrap)(result)
    }
}

// ── run_lifted: execute a lift through any Shared-domain executor ──

/// Execute a lifted computation. Shared-domain only.
pub fn run_lifted<N: 'static, H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static>(
    exec: &impl Executor<N2, R2, domain::Shared, graph::Treeish<N2>>,
    lift: &impl LiftOps<N, H, R, N2, H2, R2>,
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
pub fn run_lifted_zipped<N: 'static, H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static>(
    exec: &impl Executor<N2, R2, domain::Shared, graph::Treeish<N2>>,
    lift: &impl LiftOps<N, H, R, N2, H2, R2>,
    fold: &shared::fold::Fold<N, H, R>,
    graph: &graph::Treeish<N>,
    root: &N,
) -> (R, R2)
where R2: Clone,
{
    let lifted_fold = lift.lift_fold(fold.clone());
    let lifted_treeish = lift.lift_treeish(graph.clone());
    let lifted_root = lift.lift_root(root);
    let inner = exec.run(&lifted_fold, &lifted_treeish, &lifted_root);
    (lift.unwrap(inner.clone()), inner)
}
