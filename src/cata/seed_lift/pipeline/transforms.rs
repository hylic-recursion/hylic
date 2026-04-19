//! SeedPipeline transforms. `apply_pre_lift` is the sole primitive;
//! named sugars wrap it with shape-lift constructors (following).

use crate::ops::{ComposedLift, Lift};
use super::core::SeedPipeline;

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L> {
    /// Compose an outer lift onto the pre-lift chain. No trait bounds
    /// here — bounds surface only when `drive` is called.
    pub fn apply_pre_lift<L2>(
        self,
        outer: L2,
    ) -> SeedPipeline<N, Seed, H, R, ComposedLift<L, L2>>
    where
        N: Clone + 'static, Seed: Clone + 'static,
        H: Clone + 'static, R: Clone + 'static,
        L: Lift<N, Seed, H, R>,
        L2: Lift<L::N2, L::Seed2, L::MapH, L::MapR>,
    {
        SeedPipeline {
            grow: self.grow,
            seeds_from_node: self.seeds_from_node,
            fold: self.fold,
            pre_lift: ComposedLift::compose(self.pre_lift, outer),
        }
    }
}
