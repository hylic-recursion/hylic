//! SeedPipeline transforms. `apply_pre_lift` is the sole primitive;
//! every named sugar is a one-line wrapper over it with a shape-lift
//! from `ops::shape_lifts`.

use crate::ops::{
    ComposedLift, Lift,
    filter_seeds_lift, FilterSeedsLift,
    wrap_init_lift, WrapInitLift,
    zipmap_lift, ZipmapLift,
};
use super::core::SeedPipeline;

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L> {
    /// Compose an outer lift onto the pre-lift chain. No trait bounds
    /// — bounds surface only when `drive` is called.
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

// ── Named sugars: each is apply_pre_lift(shape_lift(...)) ──

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where
    N: Clone + 'static, Seed: Clone + 'static,
    H: Clone + 'static, R: Clone + 'static,
    L: Lift<N, Seed, H, R>,
{
    /// Filter seeds at the Edgy level (pre-grow).
    pub fn filter_seeds<P>(
        self,
        pred: P,
    ) -> SeedPipeline<N, Seed, H, R, ComposedLift<L, FilterSeedsLift<L::Seed2, P>>>
    where P: Fn(&L::Seed2) -> bool + Send + Sync + 'static,
          L::Seed2: Clone + 'static,
          L::N2: Clone + 'static,
          L::MapH: Clone + 'static,
          L::MapR: Clone + 'static,
    {
        self.apply_pre_lift(filter_seeds_lift(pred))
    }

    /// Wrap the fold's `init` closure.
    pub fn wrap_init<W>(
        self,
        wrapper: W,
    ) -> SeedPipeline<N, Seed, H, R, ComposedLift<L, WrapInitLift<L::N2, L::MapH, W>>>
    where W: Fn(&L::N2, &dyn Fn(&L::N2) -> L::MapH) -> L::MapH + Send + Sync + 'static,
          L::N2: Clone + 'static,
          L::Seed2: Clone + 'static,
          L::MapH: Clone + 'static,
          L::MapR: Clone + 'static,
    {
        self.apply_pre_lift(wrap_init_lift(wrapper))
    }

    /// Augment R with a derived value: R → (R, Extra).
    pub fn zipmap<Extra, M>(
        self,
        mapper: M,
    ) -> SeedPipeline<N, Seed, H, R, ComposedLift<L, ZipmapLift<L::MapR, Extra, M>>>
    where Extra: Clone + 'static,
          M: Fn(&L::MapR) -> Extra + Send + Sync + 'static,
          L::N2: Clone + 'static,
          L::Seed2: Clone + 'static,
          L::MapH: Clone + 'static,
          L::MapR: Clone + 'static,
    {
        self.apply_pre_lift(zipmap_lift(mapper))
    }
}
