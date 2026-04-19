//! Lift<N, H, R> — the algebra-transform type class (Phase 3).
//!
//! Three trait parameters, three associated types, one method.
//! `Seed` is method-polymorphic — the lift struct never stores
//! Seed-dependent data.

use std::sync::Arc;
use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;

// ANCHOR: lift_trait
pub trait Lift<N, H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2:   Clone + 'static;
    type MapH: Clone + 'static;
    type MapR: Clone + 'static;

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> Self::N2 + Send + Sync>,
            Treeish<Self::N2>,
            Fold<Self::N2, Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static;
}
// ANCHOR_END: lift_trait
