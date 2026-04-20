// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Local-domain Stage-2 primitive: `before_lift_local` (pre-compose
//! a type-preserving lift before the existing chain).
//!
//! Post-compose (append) lives on `then_lift.rs`; user-facing sugars
//! live on the `LiftedSugarsLocal` blanket trait in
//! `sugars/lifted_local.rs`.

use crate::domain::{Domain, Local};
use crate::ops::{ComposedLift, Lift};
use super::LiftedPipeline;
use super::super::source::TreeishSource;

impl<Base, L> LiftedPipeline<Base, L>
where
    Base: TreeishSource<Domain = Local>,
    Local: Domain<L::N2>,
    L: Lift<Local,
            <Base as TreeishSource>::N,
            <Base as TreeishSource>::H,
            <Base as TreeishSource>::R>,
    L::N2:   Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{
    pub fn before_lift_local<L0>(self, first: L0) -> LiftedPipeline<Base, ComposedLift<L0, L>>
    where L0: Lift<Local, Base::N, Base::H, Base::R>,
          Local: Domain<L0::N2>,
    {
        LiftedPipeline { base: self.base, pre_lift: ComposedLift::compose(first, self.pre_lift) }
    }
}
