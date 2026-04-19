//! apply_pre_lift — the sole Stage-2 primitive. Composes a new Lift
//! onto the pre_lift chain via ComposedLift.

use crate::ops::{ComposedLift, Lift};
use super::LiftedPipeline;

impl<N, Seed, H, R, L> LiftedPipeline<N, Seed, H, R, L> {
    pub fn apply_pre_lift<L2>(
        self,
        outer: L2,
    ) -> LiftedPipeline<N, Seed, H, R, ComposedLift<L, L2>>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        L:  Lift<N, H, R>,
        L2: Lift<L::N2, L::MapH, L::MapR>,
    {
        LiftedPipeline {
            base:     self.base,
            pre_lift: ComposedLift::compose(self.pre_lift, outer),
        }
    }
}
