//! Stage-2 sugars — each a one-line apply_pre_lift(shape_lift_ctor(...))
//! wrapper around the five shape-lifts.

use crate::ops::{
    ComposedLift, Lift,
    wrap_init_lift, WrapInitLift,
    wrap_accumulate_lift, WrapAccumulateLift,
    wrap_finalize_lift, WrapFinalizeLift,
    zipmap_lift, ZipmapLift,
    map_r_lift, MapRLift,
};
use super::LiftedPipeline;

impl<N, Seed, H, R, L> LiftedPipeline<N, Seed, H, R, L>
where
    N: Clone + 'static, Seed: Clone + 'static,
    H: Clone + 'static, R: Clone + 'static,
    L: Lift<N, H, R>,
    L::N2: Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{
    pub fn wrap_init<W>(self, wrapper: W)
        -> LiftedPipeline<N, Seed, H, R, ComposedLift<L, WrapInitLift<L::N2, L::MapH>>>
    where W: Fn(&L::N2, &dyn Fn(&L::N2) -> L::MapH) -> L::MapH + Send + Sync + 'static,
    {
        self.apply_pre_lift(wrap_init_lift(wrapper))
    }

    pub fn wrap_accumulate<W>(self, wrapper: W)
        -> LiftedPipeline<N, Seed, H, R, ComposedLift<L, WrapAccumulateLift<L::MapH, L::MapR>>>
    where W: Fn(&mut L::MapH, &L::MapR, &dyn Fn(&mut L::MapH, &L::MapR)) + Send + Sync + 'static,
    {
        self.apply_pre_lift(wrap_accumulate_lift(wrapper))
    }

    pub fn wrap_finalize<W>(self, wrapper: W)
        -> LiftedPipeline<N, Seed, H, R, ComposedLift<L, WrapFinalizeLift<L::MapH, L::MapR>>>
    where W: Fn(&L::MapH, &dyn Fn(&L::MapH) -> L::MapR) -> L::MapR + Send + Sync + 'static,
    {
        self.apply_pre_lift(wrap_finalize_lift(wrapper))
    }

    pub fn zipmap<Extra, M>(self, mapper: M)
        -> LiftedPipeline<N, Seed, H, R, ComposedLift<L, ZipmapLift<L::MapR, Extra>>>
    where Extra: Clone + 'static,
          M: Fn(&L::MapR) -> Extra + Send + Sync + 'static,
    {
        self.apply_pre_lift(zipmap_lift(mapper))
    }

    pub fn map<RNew, Fwd, Bwd>(self, forward: Fwd, backward: Bwd)
        -> LiftedPipeline<N, Seed, H, R, ComposedLift<L, MapRLift<L::MapR, RNew>>>
    where RNew: Clone + 'static,
          Fwd: Fn(&L::MapR) -> RNew + Send + Sync + 'static,
          Bwd: Fn(&RNew) -> L::MapR + Send + Sync + 'static,
    {
        self.apply_pre_lift(map_r_lift(forward, backward))
    }
}
