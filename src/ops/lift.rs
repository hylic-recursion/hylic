//! Lift — the algebra transformation type class.
//!
//! `Lift<N, Seed, H, R>` is the trait. An implementor is a concrete
//! or polymorphic transformation of a pipeline's (grow, seeds, treeish,
//! fold) tuple over the specified input types. The output types are
//! plain associated types: `N2`, `Seed2`, `MapH`, `MapR`.
//!
//! One method: `apply`, in continuation-passing style. Given the four
//! components and a continuation, the lift produces the transformed
//! four and hands them to the continuation.
//!
//! Polymorphic lifts (Identity, Explainer, ParLazy): `impl<N, Seed, H,
//! R> Lift<N, Seed, H, R> for …`. A single value serves every type
//! quadruple.
//!
//! Concrete lifts (FilterSeeds, WrapInit, Zipmap, …): the impl binds
//! specific types where the lift's stored closures demand it.
//!
//! No Send/Sync bounds here — those live in `SharedDomainLift` at the
//! domain edge.

use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;

// ANCHOR: lift_trait
pub trait Lift<N, Seed, H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2: Clone + 'static;
    type Seed2: Clone + 'static;
    type MapH: Clone + 'static;
    type MapR: Clone + 'static;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed2) -> Self::N2 + Send + Sync>,
            Edgy<Self::N2, Self::Seed2>,
            Treeish<Self::N2>,
            Fold<Self::N2, Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T;

    fn lift_root(&self, root: &N) -> Self::N2;
}
// ANCHOR_END: lift_trait
