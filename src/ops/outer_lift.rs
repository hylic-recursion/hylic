//! OuterLift: a lift defined against the output of an inner lift.
//!
//! The inner lift L1 is a trait-level parameter, making L1's GATs
//! (LiftedH<H>, LiftedR<H>) available to the outer lift's method
//! signatures. This is what enables lift composition — the outer
//! lift's lift_fold accepts exactly what the inner lift produces.

use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use super::lift::LiftOps;

/// A lift that operates on the output of an inner lift.
///
/// `lift_fold` takes `Fold<Nmid, Inner::LiftedH<H>, Inner::LiftedR<H>>`
/// — the exact output of the inner lift. H flows at the method level;
/// Inner's GATs are accessible because Inner is trait-level.
pub trait OuterLift<Inner, N: 'static, R: 'static, Nmid: Clone + 'static, N2: 'static>
where
    Inner: LiftOps<N, R, Nmid>,
{
    type LiftedH<H: Clone + 'static>: Clone + 'static;
    type LiftedR<H: Clone + 'static>: Clone + 'static;

    fn lift_treeish(&self, t: Treeish<Nmid>) -> Treeish<N2>;

    fn lift_fold<H: Clone + 'static>(
        &self,
        f: Fold<Nmid, Inner::LiftedH<H>, Inner::LiftedR<H>>,
    ) -> Fold<N2, Self::LiftedH<H>, Self::LiftedR<H>>;

    fn lift_root(&self, root: &Nmid) -> N2;
}
