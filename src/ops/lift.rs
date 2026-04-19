//! Lift — bifunctor transformation of Fold and Treeish.
//!
//! A Lift<N, N2> transforms a fold algebra from the N domain to the
//! N2 domain. Both heap (H) and result (R) are method-level — the
//! lift is a bifunctor on the (H, R) pair. This enables blanket
//! composition via ComposedLift without OuterLift boilerplate.

use crate::domain::shared;
use crate::graph;

// ANCHOR: lift_trait
pub trait Lift<N: 'static, N2: 'static> {
    type MapH<H: Clone + 'static, R: Clone + 'static>: Clone + 'static;
    type MapR<H: Clone + 'static, R: Clone + 'static>: Clone + 'static;

    fn lift_treeish(&self, t: graph::Treeish<N>) -> graph::Treeish<N2>;
    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: shared::fold::Fold<N, H, R>,
    ) -> shared::fold::Fold<N2, Self::MapH<H, R>, Self::MapR<H, R>>;
    fn lift_root(&self, root: &N) -> N2;
}
// ANCHOR_END: lift_trait
