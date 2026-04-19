//! SeedPipeline transforms. `apply_pre_lift` is the sole primitive;
//! every named sugar is a one-line wrapper over it with a shape-lift
//! from `ops::shape_lifts`. All shape-lifts store closures erased
//! (Arc<dyn Fn + Send + Sync>), so the resulting pipeline types are
//! fully nameable.

use crate::ops::{
    ComposedLift, Lift,
    filter_seeds_lift, FilterSeedsLift,
    wrap_grow_lift, WrapGrowLift,
    wrap_init_lift, WrapInitLift,
    wrap_accumulate_lift, WrapAccumulateLift,
    wrap_finalize_lift, WrapFinalizeLift,
    zipmap_lift, ZipmapLift,
    map_r_lift, MapRLift,
    contramap_node_lift, ContramapNodeLift,
    map_seed_lift, MapSeedLift,
};
use super::core::SeedPipeline;

// ── apply_pre_lift — the sole primitive ─────────────

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L> {
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

// ── Sugar methods — one-line apply_pre_lift(shape_lift_ctor(...)) ──

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where
    N: Clone + 'static, Seed: Clone + 'static,
    H: Clone + 'static, R: Clone + 'static,
    L: Lift<N, Seed, H, R>,
    L::N2: Clone + 'static,
    L::Seed2: Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{
    pub fn filter_seeds<P>(self, pred: P)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, FilterSeedsLift<L::Seed2>>>
    where P: Fn(&L::Seed2) -> bool + Send + Sync + 'static,
    {
        self.apply_pre_lift(filter_seeds_lift(pred))
    }

    pub fn wrap_grow<W>(self, wrapper: W)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, WrapGrowLift<L::N2, L::Seed2>>>
    where W: Fn(&L::Seed2, &dyn Fn(&L::Seed2) -> L::N2) -> L::N2 + Send + Sync + 'static,
    {
        self.apply_pre_lift(wrap_grow_lift(wrapper))
    }

    pub fn wrap_init<W>(self, wrapper: W)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, WrapInitLift<L::N2, L::MapH>>>
    where W: Fn(&L::N2, &dyn Fn(&L::N2) -> L::MapH) -> L::MapH + Send + Sync + 'static,
    {
        self.apply_pre_lift(wrap_init_lift(wrapper))
    }

    pub fn wrap_accumulate<W>(self, wrapper: W)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, WrapAccumulateLift<L::MapH, L::MapR>>>
    where W: Fn(&mut L::MapH, &L::MapR, &dyn Fn(&mut L::MapH, &L::MapR)) + Send + Sync + 'static,
    {
        self.apply_pre_lift(wrap_accumulate_lift(wrapper))
    }

    pub fn wrap_finalize<W>(self, wrapper: W)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, WrapFinalizeLift<L::MapH, L::MapR>>>
    where W: Fn(&L::MapH, &dyn Fn(&L::MapH) -> L::MapR) -> L::MapR + Send + Sync + 'static,
    {
        self.apply_pre_lift(wrap_finalize_lift(wrapper))
    }

    pub fn zipmap<Extra, M>(self, mapper: M)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, ZipmapLift<L::MapR, Extra>>>
    where Extra: Clone + 'static,
          M: Fn(&L::MapR) -> Extra + Send + Sync + 'static,
    {
        self.apply_pre_lift(zipmap_lift(mapper))
    }

    pub fn map<RNew, Fwd, Bwd>(self, forward: Fwd, backward: Bwd)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, MapRLift<L::MapR, RNew>>>
    where RNew: Clone + 'static,
          Fwd: Fn(&L::MapR) -> RNew + Send + Sync + 'static,
          Bwd: Fn(&RNew) -> L::MapR + Send + Sync + 'static,
    {
        self.apply_pre_lift(map_r_lift(forward, backward))
    }

    pub fn contramap_node<N2, Co, Contra>(self, co: Co, contra: Contra)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, ContramapNodeLift<L::N2, N2>>>
    where N2: Clone + 'static,
          Co: Fn(&L::N2) -> N2 + Send + Sync + 'static,
          Contra: Fn(&N2) -> L::N2 + Send + Sync + 'static,
    {
        self.apply_pre_lift(contramap_node_lift(co, contra))
    }

    pub fn map_seed<Seed2, ToNew, FromNew>(self, to_new: ToNew, from_new: FromNew)
        -> SeedPipeline<N, Seed, H, R, ComposedLift<L, MapSeedLift<L::Seed2, Seed2>>>
    where Seed2: Clone + 'static,
          ToNew: Fn(&L::Seed2) -> Seed2 + Send + Sync + 'static,
          FromNew: Fn(&Seed2) -> L::Seed2 + Send + Sync + 'static,
    {
        self.apply_pre_lift(map_seed_lift(to_new, from_new))
    }
}
