//! SeedPipeline transforms — `apply_pre_lift` is the sole primitive.
//! Named sugar methods wrap this with shape-lift constructors.

use crate::ops::{ComposedLift, Lift};
use super::core::SeedPipeline;

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L> {
    /// Compose an outer lift onto the pre-lift chain. No trait bounds
    /// — pure type-reshape; Shared-domain bounds only surface when
    /// `drive` is actually called.
    pub fn apply_pre_lift<L2: Lift>(
        self,
        outer: L2,
    ) -> SeedPipeline<N, Seed, H, R, ComposedLift<L, L2>> {
        SeedPipeline {
            grow: self.grow,
            seeds_from_node: self.seeds_from_node,
            fold: self.fold,
            pre_lift: ComposedLift::compose(self.pre_lift, outer),
        }
    }
}
