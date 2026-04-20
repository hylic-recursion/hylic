//! Stage-2 algebra sugars on Local-bound LiftedPipelines. Requires
//! only `TreeishSource` on Base.

use crate::domain::{Domain, Local};
use crate::ops::{ComposedLift, Lift, ShapeLift};
use super::LiftedPipeline;
use super::super::source::TreeishSource;
use crate::prelude::explainer::{ExplainerHeap, ExplainerResult};

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
        self.then_lift(Local::wrap_init_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_accumulate_local<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&mut L::MapH, &L::MapR, &dyn Fn(&mut L::MapH, &L::MapR)) + 'static,
    {
        self.then_lift(Local::wrap_accumulate_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_finalize_local<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::MapH, &dyn Fn(&L::MapH) -> L::MapR) -> L::MapR + 'static,
    {
        self.then_lift(Local::wrap_finalize_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn zipmap_local<Extra, M>(self, mapper: M)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, (L::MapR, Extra)>>>
    where Extra: Clone + 'static,
          M: Fn(&L::MapR) -> Extra + 'static,
    {
        self.then_lift(Local::zipmap_lift::<L::N2, L::MapH, L::MapR, Extra, _>(mapper))
    }

    pub fn map_r_bi_local<RNew, Fwd, Bwd>(self, forward: Fwd, backward: Bwd)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, RNew>>>
    where RNew: Clone + 'static,
          Fwd: Fn(&L::MapR) -> RNew + 'static,
          Bwd: Fn(&RNew) -> L::MapR + 'static,
    {
        self.then_lift(Local::map_r_bi_lift::<L::N2, L::MapH, L::MapR, RNew, _, _>(forward, backward))
    }

    // ── treeish-side sugars (Stage-2 mirrors of Stage-1) ─────

    pub fn filter_edges_local<P>(self, pred: P)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where P: Fn(&L::N2) -> bool + 'static,
    {
        self.then_lift(Local::filter_edges_lift::<L::N2, L::MapH, L::MapR, _>(pred))
    }

    pub fn wrap_visit_local<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::N2, &mut dyn FnMut(&L::N2),
                &dyn Fn(&L::N2, &mut dyn FnMut(&L::N2)))
            + 'static,
    {
        self.then_lift(Local::wrap_visit_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn memoize_by_local<K, KeyFn>(self, key_fn: KeyFn)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where K: Eq + std::hash::Hash + 'static,
          KeyFn: Fn(&L::N2) -> K + 'static,
    {
        self.then_lift(Local::memoize_by_lift::<L::N2, L::MapH, L::MapR, K, _>(key_fn))
    }

    // ── N-change (Stage-2 mirror of Stage-1 map_node_bi) ──

    pub fn map_node_bi_local<NewN, Co, Contra>(self, co: Co, contra: Contra)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR, NewN, L::MapH, L::MapR>>>
    where NewN: Clone + 'static,
          Local: Domain<NewN>,
          Co:     Fn(&L::N2) -> NewN + Clone + 'static,
          Contra: Fn(&NewN) -> L::N2 + Clone + 'static,
    {
        self.then_lift(Local::map_n_bi_lift::<L::N2, L::MapH, L::MapR, NewN, _, _>(co, contra))
    }

    // ── explainer ──

    pub fn explain_local(self)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Local, L::N2, L::MapH, L::MapR,
                              L::N2,
                              ExplainerHeap<L::N2, L::MapH, ExplainerResult<L::N2, L::MapH, L::MapR>>,
                              ExplainerResult<L::N2, L::MapH, L::MapR>>>>
    {
        self.then_lift(Local::explainer_lift::<L::N2, L::MapH, L::MapR>())
    }

    // ── before_lift ──

    pub fn before_lift_local<L0>(self, first: L0) -> LiftedPipeline<Base, ComposedLift<L0, L>>
    where L0: Lift<Local, Base::N, Base::H, Base::R>,
          Local: Domain<L0::N2>,
    {
        LiftedPipeline { base: self.base, pre_lift: ComposedLift::compose(first, self.pre_lift) }
    }
}
