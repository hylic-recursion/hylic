//! Lift — CPS algebra transform, domain-neutral.
//!
//! A Lift receives the four components of a pipeline's algebra +
//! coalgebra (grow, seeds, treeish, fold) assembled on one side and
//! yields their lifted versions to a user-supplied continuation.
//! One method: `apply`. Composition is CPS nesting (ComposedLift).
//!
//! This trait carries NO Send/Sync bounds — those live at the domain
//! layer (see `SharedDomainLift` for the Shared-domain bundle).

use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;

// ANCHOR: lift_trait
pub trait Lift {
    type N2<N: Clone + 'static>: Clone + 'static;
    type Seed2<Seed: Clone + 'static>: Clone + 'static;
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>: Clone + 'static;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>: Clone + 'static;

    fn apply<N, Seed, H, R, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed2<Seed>) -> Self::N2<N> + Send + Sync>,
            Edgy<Self::N2<N>, Self::Seed2<Seed>>,
            Treeish<Self::N2<N>>,
            Fold<Self::N2<N>, Self::MapH<N, H, R>, Self::MapR<N, H, R>>,
        ) -> T,
    ) -> T
    where N: Clone + 'static, Seed: Clone + 'static, H: Clone + 'static, R: Clone + 'static;

    fn lift_root<N: Clone + 'static>(&self, root: &N) -> Self::N2<N>;
}
// ANCHOR_END: lift_trait
