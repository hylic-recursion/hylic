//! apply_pre_lift — the sole Stage-2 primitive. Composes a new Lift
//! onto the pre_lift chain via ComposedLift. Generic over any base
//! `PipelineSource`.

use crate::ops::{ComposedLift, Lift};
use super::LiftedPipeline;
use super::super::source::PipelineSource;

impl<Base, L> LiftedPipeline<Base, L>
where Base: PipelineSource,
      L: Lift<Base::N, Base::H, Base::R>,
{
    pub fn apply_pre_lift<L2>(
        self,
        outer: L2,
    ) -> LiftedPipeline<Base, ComposedLift<L, L2>>
    where L2: Lift<L::N2, L::MapH, L::MapR>,
    {
        LiftedPipeline {
            base:     self.base,
            pre_lift: ComposedLift::compose(self.pre_lift, outer),
        }
    }
}
