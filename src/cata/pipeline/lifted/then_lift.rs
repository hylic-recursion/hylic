//! then_lift — the sole Stage-2 primitive. Composes a new Lift
//! onto the pre_lift chain via ComposedLift. Requires only
//! `TreeishSource` on the base — Seed-agnostic composition.

use crate::domain::Domain;
use crate::ops::{ComposedLift, Lift};
use super::LiftedPipeline;
use super::super::source::TreeishSource;

impl<Base, L> LiftedPipeline<Base, L>
where Base: TreeishSource,
      <Base as TreeishSource>::Domain: Domain<L::N2>,
      L: Lift<<Base as TreeishSource>::Domain,
              <Base as TreeishSource>::N,
              <Base as TreeishSource>::H,
              <Base as TreeishSource>::R>,
{
    pub fn then_lift<L2>(
        self,
        outer: L2,
    ) -> LiftedPipeline<Base, ComposedLift<L, L2>>
    where <Base as TreeishSource>::Domain: Domain<L2::N2>,
          L2: Lift<<Base as TreeishSource>::Domain, L::N2, L::MapH, L::MapR>,
    {
        LiftedPipeline {
            base:     self.base,
            pre_lift: ComposedLift::compose(self.pre_lift, outer),
        }
    }
}
