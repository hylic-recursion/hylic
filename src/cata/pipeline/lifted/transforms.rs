//! Stage-2 algebra sugars — one-liners over `apply_pre_lift` with a
//! Shared-domain shape-lift constructor. Generic over the base source.
//! Pinned to Shared pending Phase 5/5.

use crate::domain::Shared;
use crate::ops::{ComposedLift, Lift, ShapeLift};
use super::LiftedPipeline;
use super::super::source::PipelineSource;

impl<Base, L> LiftedPipeline<Base, L>
where
    Base: PipelineSource,
    L: Lift<Shared, Base::N, Base::H, Base::R>,
    L::N2:   Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{
    pub fn wrap_init<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::N2, &dyn Fn(&L::N2) -> L::MapH) -> L::MapH + Send + Sync + 'static,
    {
        self.apply_pre_lift(Shared::wrap_init_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_accumulate<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&mut L::MapH, &L::MapR, &dyn Fn(&mut L::MapH, &L::MapR)) + Send + Sync + 'static,
    {
        self.apply_pre_lift(Shared::wrap_accumulate_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_finalize<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::MapH, &dyn Fn(&L::MapH) -> L::MapR) -> L::MapR + Send + Sync + 'static,
    {
        self.apply_pre_lift(Shared::wrap_finalize_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn zipmap<Extra, M>(self, mapper: M)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, (L::MapR, Extra)>>>
    where Extra: Clone + 'static,
          M: Fn(&L::MapR) -> Extra + Send + Sync + 'static,
    {
        self.apply_pre_lift(Shared::zipmap_lift::<L::N2, L::MapH, L::MapR, Extra, _>(mapper))
    }

    pub fn map<RNew, Fwd, Bwd>(self, forward: Fwd, backward: Bwd)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, RNew>>>
    where RNew: Clone + 'static,
          Fwd: Fn(&L::MapR) -> RNew + Send + Sync + 'static,
          Bwd: Fn(&RNew) -> L::MapR + Send + Sync + 'static,
    {
        self.apply_pre_lift(Shared::map_r_lift::<L::N2, L::MapH, L::MapR, RNew, _, _>(forward, backward))
    }
}
