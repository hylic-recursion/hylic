//! Stage-2 algebra sugars on Shared-bound LiftedPipelines. Requires
//! only `TreeishSource` on Base — Seed-agnostic.

use crate::domain::{Domain, Shared};
use crate::ops::{ComposedLift, Lift, ShapeLift};
use super::LiftedPipeline;
use super::super::source::TreeishSource;
use crate::prelude::explainer::{ExplainerHeap, ExplainerResult};

impl<Base, L> LiftedPipeline<Base, L>
where
    Base: TreeishSource<Domain = Shared>,
    Shared: Domain<L::N2>,
    L: Lift<Shared,
            <Base as TreeishSource>::N,
            <Base as TreeishSource>::H,
            <Base as TreeishSource>::R>,
    L::N2:   Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{
    pub fn wrap_init<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::N2, &dyn Fn(&L::N2) -> L::MapH) -> L::MapH + Send + Sync + 'static,
    {
        self.then_lift(Shared::wrap_init_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_accumulate<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&mut L::MapH, &L::MapR, &dyn Fn(&mut L::MapH, &L::MapR)) + Send + Sync + 'static,
    {
        self.then_lift(Shared::wrap_accumulate_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn wrap_finalize<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::MapH, &dyn Fn(&L::MapH) -> L::MapR) -> L::MapR + Send + Sync + 'static,
    {
        self.then_lift(Shared::wrap_finalize_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn zipmap<Extra, M>(self, mapper: M)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, (L::MapR, Extra)>>>
    where Extra: Clone + 'static,
          M: Fn(&L::MapR) -> Extra + Send + Sync + 'static,
    {
        self.then_lift(Shared::zipmap_lift::<L::N2, L::MapH, L::MapR, Extra, _>(mapper))
    }

    pub fn map_r_bi<RNew, Fwd, Bwd>(self, forward: Fwd, backward: Bwd)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, RNew>>>
    where RNew: Clone + 'static,
          Fwd: Fn(&L::MapR) -> RNew + Send + Sync + 'static,
          Bwd: Fn(&RNew) -> L::MapR + Send + Sync + 'static,
    {
        self.then_lift(Shared::map_r_bi_lift::<L::N2, L::MapH, L::MapR, RNew, _, _>(forward, backward))
    }

    // ── treeish-side sugars (Stage-2 mirrors of Stage-1) ─────

    pub fn filter_edges<P>(self, pred: P)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where P: Fn(&L::N2) -> bool + Send + Sync + 'static,
    {
        self.then_lift(Shared::filter_edges_lift::<L::N2, L::MapH, L::MapR, _>(pred))
    }

    pub fn wrap_visit<W>(self, wrapper: W)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where W: Fn(&L::N2, &mut dyn FnMut(&L::N2),
                &dyn Fn(&L::N2, &mut dyn FnMut(&L::N2)))
            + Send + Sync + 'static,
    {
        self.then_lift(Shared::wrap_visit_lift::<L::N2, L::MapH, L::MapR, _>(wrapper))
    }

    pub fn memoize_by<K, KeyFn>(self, key_fn: KeyFn)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, L::N2, L::MapH, L::MapR>>>
    where L::N2: Send + Sync,
          K: Eq + std::hash::Hash + Send + Sync + 'static,
          KeyFn: Fn(&L::N2) -> K + Send + Sync + 'static,
    {
        self.then_lift(Shared::memoize_by_lift::<L::N2, L::MapH, L::MapR, K, _>(key_fn))
    }

    // ── N-change (Stage-2 mirror of Stage-1 map_node_bi) ──

    pub fn map_node_bi<NewN, Co, Contra>(self, co: Co, contra: Contra)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR, NewN, L::MapH, L::MapR>>>
    where NewN: Clone + 'static,
          Shared: Domain<NewN>,
          Co:     Fn(&L::N2) -> NewN + Clone + Send + Sync + 'static,
          Contra: Fn(&NewN) -> L::N2 + Clone + Send + Sync + 'static,
    {
        self.then_lift(Shared::map_n_bi_lift::<L::N2, L::MapH, L::MapR, NewN, _, _>(co, contra))
    }

    // ── explainer ──

    pub fn explain(self)
        -> LiftedPipeline<Base, ComposedLift<L, ShapeLift<Shared, L::N2, L::MapH, L::MapR,
                              L::N2,
                              ExplainerHeap<L::N2, L::MapH, ExplainerResult<L::N2, L::MapH, L::MapR>>,
                              ExplainerResult<L::N2, L::MapH, L::MapR>>>>
    where L::N2: Send + Sync, L::MapH: Send + Sync, L::MapR: Send + Sync,
    {
        self.then_lift(Shared::explainer_lift::<L::N2, L::MapH, L::MapR>())
    }

    // ── before_lift ──

    pub fn before_lift<L0>(self, first: L0) -> LiftedPipeline<Base, ComposedLift<L0, L>>
    where L0: Lift<Shared, Base::N, Base::H, Base::R>,
          Shared: Domain<L0::N2>,
    {
        LiftedPipeline { base: self.base, pre_lift: ComposedLift::compose(first, self.pre_lift) }
    }
}
