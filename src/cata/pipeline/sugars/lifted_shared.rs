//! Blanket Stage-2 sugars for the Shared domain.
//!
//! `LiftedSugarsShared<N, H, R>` is implemented for any pipeline
//! yielding `(treeish, fold)` over `(N, H, R)` in Shared. Stage-1
//! pipelines (`SeedPipeline`, `TreeishPipeline`) auto-lift first;
//! `LiftedPipeline` composes at the tip. Each sugar method is written
//! **once** and inherited by every implementer.
//!
//! Trait type parameters `N, H, R` are used instead of `Self::N`/
//! `Self::H`/`Self::R` projections — this sidesteps Rust's projection-
//! normalisation rules that blocked the earlier attempt (see
//! `KB/.plans/pipeline-surface-finish/PLAN.md` § Stage E note).

use crate::cata::pipeline::lifted::LiftedPipeline;
use crate::cata::pipeline::seed::SeedPipeline;
use crate::cata::pipeline::treeish::TreeishPipeline;
use crate::cata::pipeline::source::TreeishSource;
use crate::domain::{Domain, Shared};
use crate::ops::{ComposedLift, IdentityLift, Lift, ShapeLift};
use crate::prelude::explainer::{ExplainerHeap, ExplainerResult};

pub trait LiftedSugarsShared<N, H, R>:
    TreeishSource<Domain = Shared, N = N, H = H, R = R> + Sized
where
    N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type With<L2>: TreeishSource<Domain = Shared>
    where L2: Lift<Shared, N, H, R>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>;

    /// Sole primitive: append a lift to the chain.
    fn then_lift<L2>(self, l: L2) -> Self::With<L2>
    where L2: Lift<Shared, N, H, R>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>;

    // ── fold-side sugars ─────────────────────────────────────

    fn wrap_init<W>(self, wrapper: W) -> Self::With<ShapeLift<Shared, N, H, R, N, H, R>>
    where W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    { self.then_lift(Shared::wrap_init_lift::<N, H, R, _>(wrapper)) }

    fn wrap_accumulate<W>(self, wrapper: W) -> Self::With<ShapeLift<Shared, N, H, R, N, H, R>>
    where W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    { self.then_lift(Shared::wrap_accumulate_lift::<N, H, R, _>(wrapper)) }

    fn wrap_finalize<W>(self, wrapper: W) -> Self::With<ShapeLift<Shared, N, H, R, N, H, R>>
    where W: Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    { self.then_lift(Shared::wrap_finalize_lift::<N, H, R, _>(wrapper)) }

    fn zipmap<Extra, M>(self, mapper: M) -> Self::With<ShapeLift<Shared, N, H, R, N, H, (R, Extra)>>
    where Extra: Clone + 'static,
          M: Fn(&R) -> Extra + Send + Sync + 'static,
    { self.then_lift(Shared::zipmap_lift::<N, H, R, Extra, _>(mapper)) }

    fn map_r_bi<RNew, Fwd, Bwd>(self, forward: Fwd, backward: Bwd)
        -> Self::With<ShapeLift<Shared, N, H, R, N, H, RNew>>
    where RNew: Clone + 'static,
          Fwd: Fn(&R) -> RNew + Send + Sync + 'static,
          Bwd: Fn(&RNew) -> R + Send + Sync + 'static,
    { self.then_lift(Shared::map_r_bi_lift::<N, H, R, RNew, _, _>(forward, backward)) }

    // ── treeish-side sugars ──────────────────────────────────

    fn filter_edges<P>(self, pred: P) -> Self::With<ShapeLift<Shared, N, H, R, N, H, R>>
    where P: Fn(&N) -> bool + Send + Sync + 'static,
    { self.then_lift(Shared::filter_edges_lift::<N, H, R, _>(pred)) }

    fn wrap_visit<W>(self, wrapper: W) -> Self::With<ShapeLift<Shared, N, H, R, N, H, R>>
    where W: Fn(&N, &mut dyn FnMut(&N), &dyn Fn(&N, &mut dyn FnMut(&N))) + Send + Sync + 'static,
    { self.then_lift(Shared::wrap_visit_lift::<N, H, R, _>(wrapper)) }

    fn memoize_by<K, KeyFn>(self, key_fn: KeyFn)
        -> Self::With<ShapeLift<Shared, N, H, R, N, H, R>>
    where N: Send + Sync,
          K: Eq + std::hash::Hash + Send + Sync + 'static,
          KeyFn: Fn(&N) -> K + Send + Sync + 'static,
    { self.then_lift(Shared::memoize_by_lift::<N, H, R, K, _>(key_fn)) }

    // ── N-change ─────────────────────────────────────────────

    fn map_node_bi<NewN, Co, Contra>(self, co: Co, contra: Contra)
        -> Self::With<ShapeLift<Shared, N, H, R, NewN, H, R>>
    where NewN: Clone + 'static,
          Shared: Domain<NewN>,
          Co:     Fn(&N) -> NewN + Clone + Send + Sync + 'static,
          Contra: Fn(&NewN) -> N + Clone + Send + Sync + 'static,
    { self.then_lift(Shared::map_n_bi_lift::<N, H, R, NewN, _, _>(co, contra)) }

    // ── explainer ────────────────────────────────────────────

    fn explain(self) -> Self::With<ShapeLift<Shared, N, H, R, N,
                                   ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
                                   ExplainerResult<N, H, R>>>
    where N: Send + Sync, H: Send + Sync, R: Send + Sync,
    { self.then_lift(Shared::explainer_lift::<N, H, R>()) }
}

// ── Impl 1: SeedPipeline — auto-lifts first ────────────────────

impl<N, Seed, H, R> LiftedSugarsShared<N, H, R> for SeedPipeline<Shared, N, Seed, H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type With<L2> = LiftedPipeline<Self, ComposedLift<IdentityLift, L2>>
    where L2: Lift<Shared, N, H, R>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>;

    fn then_lift<L2>(self, l: L2) -> Self::With<L2>
    where L2: Lift<Shared, N, H, R>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>,
    {
        LiftedPipeline::then_lift(self.lift(), l)
    }
}

// ── Impl 2: TreeishPipeline — auto-lifts first ─────────────────

impl<N, H, R> LiftedSugarsShared<N, H, R> for TreeishPipeline<Shared, N, H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type With<L2> = LiftedPipeline<Self, ComposedLift<IdentityLift, L2>>
    where L2: Lift<Shared, N, H, R>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>;

    fn then_lift<L2>(self, l: L2) -> Self::With<L2>
    where L2: Lift<Shared, N, H, R>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>,
    {
        LiftedPipeline::then_lift(self.lift(), l)
    }
}

// ── Impl 3: LiftedPipeline — compose at the tip ────────────────

impl<Base, L> LiftedSugarsShared<L::N2, L::MapH, L::MapR> for LiftedPipeline<Base, L>
where Base: TreeishSource<Domain = Shared>,
      Shared: Domain<L::N2>,
      L: Lift<Shared, Base::N, Base::H, Base::R>,
      L::N2:   Clone + 'static,
      L::MapH: Clone + 'static,
      L::MapR: Clone + 'static,
{
    type With<L2> = LiftedPipeline<Base, ComposedLift<L, L2>>
    where L2: Lift<Shared, L::N2, L::MapH, L::MapR>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>;

    fn then_lift<L2>(self, l: L2) -> Self::With<L2>
    where L2: Lift<Shared, L::N2, L::MapH, L::MapR>,
          L2::N2:   Clone + 'static,
          L2::MapH: Clone + 'static,
          L2::MapR: Clone + 'static,
          Shared:   Domain<L2::N2>,
    {
        LiftedPipeline::then_lift(self, l)
    }
}
