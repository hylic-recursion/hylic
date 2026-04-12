//! Pipeline transforms: from the general (map_constituents) to the
//! specific (wrap_grow, filter_seeds, etc). Each specific transform
//! is derivable from map_constituents.

use std::sync::Arc;
use crate::domain::shared;
use crate::graph::Edgy;
use super::pipeline::SeedPipeline;

// ── The general transform ───────────────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static> SeedPipeline<N, Seed, H, R> {
    /// The most general pipeline transform. Maps each constituent
    /// through a user-provided function. All type parameters can change.
    pub fn map_constituents<N2: 'static, Seed2: 'static, H2: 'static, R2: 'static>(
        &self,
        map_grow: impl FnOnce(Arc<dyn Fn(&Seed) -> N + Send + Sync>) -> Arc<dyn Fn(&Seed2) -> N2 + Send + Sync>,
        map_seeds: impl FnOnce(Edgy<N, Seed>) -> Edgy<N2, Seed2>,
        map_fold: impl FnOnce(shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, H2, R2>,
    ) -> SeedPipeline<N2, Seed2, H2, R2> {
        SeedPipeline {
            grow: map_grow(self.grow.clone()),
            seeds_from_node: map_seeds(self.seeds_from_node.clone()),
            fold: map_fold(self.fold.clone()),
        }
    }
}

// ── Type-changing transforms (derived from map_constituents) ──

impl<N: 'static, Seed: 'static, H: 'static, R: 'static> SeedPipeline<N, Seed, H, R> {
    /// Change the node type. Bidirectional: N is invariant.
    pub fn contramap_node<N2: 'static>(
        &self,
        co: impl Fn(&N) -> N2 + Send + Sync + 'static,
        contra: impl Fn(&N2) -> N + Send + Sync + 'static,
    ) -> SeedPipeline<N2, Seed, H, R> {
        let co = Arc::new(co);
        let contra = Arc::new(contra);
        self.map_constituents(
            { let co = co.clone(); move |g| Arc::new(move |s: &Seed| co(&g(s))) },
            { let contra = contra.clone(); move |e| e.contramap(move |n: &N2| contra(n)) },
            { let contra = contra.clone(); move |f| f.contramap(move |n: &N2| contra(n)) },
        )
    }

    /// Change the seed type. Bidirectional: Seed is covariant in
    /// seeds_from_node, contravariant in grow.
    pub fn map_seed<Seed2: 'static>(
        &self,
        to_new: impl Fn(&Seed) -> Seed2 + Send + Sync + 'static,
        from_new: impl Fn(&Seed2) -> Seed + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed2, H, R> {
        let from_new = Arc::new(from_new);
        self.map_constituents(
            { let from_new = from_new.clone(); move |g| Arc::new(move |s: &Seed2| g(&from_new(s))) },
            move |e| e.map(to_new),
            |f| f,
        )
    }
}

// ── R-type transforms ───────────────────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static> SeedPipeline<N, Seed, H, R> {
    pub fn zipmap<Extra: 'static>(
        &self,
        mapper: impl Fn(&R) -> Extra + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, (R, Extra)>
    where R: Clone,
    {
        self.map_constituents(|g| g, |e| e, |f| f.zipmap(mapper))
    }

    pub fn map<RNew: 'static>(
        &self,
        mapper: impl Fn(&R) -> RNew + Send + Sync + 'static,
        backmapper: impl Fn(&RNew) -> R + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, RNew> {
        self.map_constituents(|g| g, |e| e, |f| f.map(mapper, backmapper))
    }
}

// ── Phase wrapping ──────────────────────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static> SeedPipeline<N, Seed, H, R> {
    pub fn wrap_grow(
        &self,
        wrapper: impl Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + Send + Sync + 'static,
    ) -> Self {
        let old = self.grow.clone();
        self.map_constituents(
            move |_| Arc::new(move |s: &Seed| wrapper(s, &|s| old(s))),
            |e| e,
            |f| f,
        )
    }

    pub fn filter_seeds(
        &self,
        pred: impl Fn(&Seed) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, move |e| e.filter(pred), |f| f)
    }

    pub fn wrap_init(
        &self,
        wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_init(wrapper))
    }

    pub fn wrap_accumulate(
        &self,
        wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_accumulate(wrapper))
    }

    pub fn wrap_finalize(
        &self,
        wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_finalize(wrapper))
    }
}
