//! Stage-2 algebra sugars on Local-bound LiftedPipelines. Requires
//! only `TreeishSource` on Base.

use crate::domain::{Domain, Local};
use crate::ops::{ComposedLift, Lift, ShapeLift};
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
    pub fn wrap_init_local<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::N2, &dyn Fn(&L::N2) -> L::MapH) -> L::MapH + 'static,
    {
        self.apply_pre_lift(Local::wrap_init_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_accumulate_local<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&mut L::MapH, &L::MapR, &dyn Fn(&mut L::MapH, &L::MapR)) + 'static,
    {
        self.apply_pre_lift(Local::wrap_accumulate_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_finalize_local<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::MapH, &dyn Fn(&L::MapH) -> L::MapR) -> L::MapR + 'static,
    {
        self.apply_pre_lift(Local::wrap_finalize_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn zipmap_local<Extra, M>(self, mapper: M)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, (L::MapR, Extra)>>>
    where Extra: Clone + 'static,
          M: Fn(&L::MapR) -> Extra + 'static,
    {
        self.apply_pre_lift(Local::zipmap_lift::<L::N2, L::MapH, L::MapR, Extra, _>(mapper))
    }

    pub fn map_local<RNew, Fwd, Bwd>(self, forward: Fwd, backward: Bwd)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, RNew>>>
    where RNew: Clone + 'static,
          Fwd: Fn(&L::MapR) -> RNew + 'static,
          Bwd: Fn(&RNew) -> L::MapR + 'static,
    {
        self.apply_pre_lift(Local::map_r_lift::<L::N2, L::MapH, L::MapR, RNew, _, _>(forward, backward))
    }
}
