//! LiftOps — forward transformation of Fold and Treeish.
//!
//! Transforms Shared-domain Fold and Treeish into a different type
//! domain. The lifted heap and result types are GATs (`LiftedH<H>`,
//! `LiftedR<H>`). H is bounded by `Clone + 'static` — lifts copy
//! heap state between phases (tracing, lazy evaluation, seed relay).
//! Folds with non-Clone heaps can still run directly through executors
//! but cannot be lifted.

use crate::domain::shared;
use crate::graph;

// ANCHOR: liftops_trait
pub trait LiftOps<N: 'static, R: 'static, N2: 'static> {
    type LiftedH<H: Clone + 'static>: 'static;
    type LiftedR<H: Clone + 'static>: 'static;

    fn lift_treeish(&self, t: graph::Treeish<N>) -> graph::Treeish<N2>;
    fn lift_fold<H: Clone + 'static>(
        &self, f: shared::fold::Fold<N, H, R>,
    ) -> shared::fold::Fold<N2, Self::LiftedH<H>, Self::LiftedR<H>>;
    fn lift_root(&self, root: &N) -> N2;
}
// ANCHOR_END: liftops_trait
