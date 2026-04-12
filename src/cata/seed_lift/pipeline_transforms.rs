//! Pipeline transforms. map_constituents is the general mechanism.
//! All specific transforms derive from it.

use std::sync::Arc;
use crate::domain::shared;
use crate::graph::Edgy;
use crate::ops::{ComposedLift, OuterLift, LiftOps};
use super::pipeline::SeedPipeline;

// ── The general constituent transform ───────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, L> SeedPipeline<N, Seed, H, R, L> {
    pub fn map_constituents<N2: 'static, Seed2: 'static, H2: 'static, R2: 'static, L2>(
        &self,
        map_grow: impl FnOnce(Arc<dyn Fn(&Seed) -> N + Send + Sync>) -> Arc<dyn Fn(&Seed2) -> N2 + Send + Sync>,
        map_seeds: impl FnOnce(Edgy<N, Seed>) -> Edgy<N2, Seed2>,
        map_fold: impl FnOnce(shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, H2, R2>,
        map_pre_lift: impl FnOnce(L) -> L2,
    ) -> SeedPipeline<N2, Seed2, H2, R2, L2>
    where L: Clone,
    {
        SeedPipeline {
            grow: map_grow(self.grow.clone()),
            seeds_from_node: map_seeds(self.seeds_from_node.clone()),
            fold: map_fold(self.fold.clone()),
            pre_lift: map_pre_lift(self.pre_lift.clone()),
        }
    }
}

// ── Type-changing constituent transforms ────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, L: Clone> SeedPipeline<N, Seed, H, R, L> {
    pub fn contramap_node<N2: 'static>(
        &self,
        co: impl Fn(&N) -> N2 + Send + Sync + 'static,
        contra: impl Fn(&N2) -> N + Send + Sync + 'static,
    ) -> SeedPipeline<N2, Seed, H, R, L> {
        let co = Arc::new(co);
        let contra = Arc::new(contra);
        self.map_constituents(
            { let co = co.clone(); move |g| Arc::new(move |s: &Seed| co(&g(s))) },
            { let contra = contra.clone(); move |e| e.contramap(move |n: &N2| contra(n)) },
            { let contra = contra.clone(); move |f| f.contramap(move |n: &N2| contra(n)) },
            |l| l,
        )
    }

    pub fn map_seed<Seed2: 'static>(
        &self,
        to_new: impl Fn(&Seed) -> Seed2 + Send + Sync + 'static,
        from_new: impl Fn(&Seed2) -> Seed + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed2, H, R, L> {
        let from_new = Arc::new(from_new);
        self.map_constituents(
            { let from_new = from_new.clone(); move |g| Arc::new(move |s: &Seed2| g(&from_new(s))) },
            move |e| e.map(to_new),
            |f| f,
            |l| l,
        )
    }
}

// ── R-type transforms ───────────────────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, L: Clone> SeedPipeline<N, Seed, H, R, L> {
    pub fn zipmap<Extra: 'static>(
        &self,
        mapper: impl Fn(&R) -> Extra + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, (R, Extra), L>
    where R: Clone,
    {
        self.map_constituents(|g| g, |e| e, |f| f.zipmap(mapper), |l| l)
    }

    pub fn map<RNew: 'static>(
        &self,
        mapper: impl Fn(&R) -> RNew + Send + Sync + 'static,
        backmapper: impl Fn(&RNew) -> R + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, RNew, L> {
        self.map_constituents(|g| g, |e| e, |f| f.map(mapper, backmapper), |l| l)
    }
}

// ── Phase wrapping ──────────────────────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, L: Clone> SeedPipeline<N, Seed, H, R, L> {
    pub fn wrap_grow(
        &self,
        wrapper: impl Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + Send + Sync + 'static,
    ) -> Self {
        let old = self.grow.clone();
        self.map_constituents(
            move |_| Arc::new(move |s: &Seed| wrapper(s, &|s| old(s))),
            |e| e, |f| f, |l| l,
        )
    }

    pub fn filter_seeds(
        &self,
        pred: impl Fn(&Seed) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, move |e| e.filter(pred), |f| f, |l| l)
    }

    pub fn wrap_init(
        &self,
        wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_init(wrapper), |l| l)
    }

    pub fn wrap_accumulate(
        &self,
        wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_accumulate(wrapper), |l| l)
    }

    pub fn wrap_finalize(
        &self,
        wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_finalize(wrapper), |l| l)
    }
}

// ── Pre-lift composition ────────────────────────────

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where
    N: Clone + 'static,
    Seed: 'static,
    H: 'static,
    R: Clone + 'static,
    L: LiftOps<N, R, N> + Clone,
{
    /// Compose an outer lift onto the pre-lift chain. The outer lift
    /// operates on the pre-lift's output — its lift_fold receives
    /// the fold that L already transformed.
    pub fn apply_pre_lift<L2>(
        &self,
        outer: L2,
    ) -> SeedPipeline<N, Seed, H, R, ComposedLift<L, L2, N>>
    where
        L2: OuterLift<L, N, R, N, N>,
    {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.clone(),
            pre_lift: ComposedLift::compose(self.pre_lift.clone(), outer),
        }
    }
}
