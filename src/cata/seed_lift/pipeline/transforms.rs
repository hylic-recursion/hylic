//! SeedPipeline transforms — every one is a one-line by-value wrapper
//! over `map_constituents`. Taking `self` consumes the pipeline; the
//! fluent chain reads identically at the call site but never clones
//! the pre-lift implicitly.
//!
//! Categories:
//!   - Type-preserving phase wrapping: wrap_grow, filter_seeds,
//!     wrap_init, wrap_accumulate, wrap_finalize
//!   - R-type transforms: map, zipmap
//!   - Type-changing constituent transforms: contramap_node, map_seed
//!   - Lift composition: apply_pre_lift

use std::sync::Arc;
use crate::ops::{ComposedLift, Lift};
use super::core::SeedPipeline;

// ── Type-changing constituent transforms ────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, Nt: 'static, L>
    SeedPipeline<N, Seed, H, R, Nt, L>
{
    pub fn contramap_node<N2: 'static>(
        self,
        co: impl Fn(&N) -> N2 + Send + Sync + 'static,
        contra: impl Fn(&N2) -> N + Send + Sync + 'static,
    ) -> SeedPipeline<N2, Seed, H, R, N2, L> {
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
        self,
        to_new: impl Fn(&Seed) -> Seed2 + Send + Sync + 'static,
        from_new: impl Fn(&Seed2) -> Seed + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed2, H, R, Nt, L> {
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

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, Nt: 'static, L>
    SeedPipeline<N, Seed, H, R, Nt, L>
{
    pub fn zipmap<Extra: 'static>(
        self,
        mapper: impl Fn(&R) -> Extra + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, (R, Extra), Nt, L>
    where R: Clone,
    {
        self.map_constituents(|g| g, |e| e, |f| f.zipmap(mapper), |l| l)
    }

    pub fn map<RNew: 'static>(
        self,
        mapper: impl Fn(&R) -> RNew + Send + Sync + 'static,
        backmapper: impl Fn(&RNew) -> R + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, RNew, Nt, L> {
        self.map_constituents(|g| g, |e| e, |f| f.map(mapper, backmapper), |l| l)
    }
}

// ── Type-preserving phase wrapping ──────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, Nt: 'static, L>
    SeedPipeline<N, Seed, H, R, Nt, L>
{
    pub fn wrap_grow(
        self,
        wrapper: impl Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + Send + Sync + 'static,
    ) -> Self {
        let old = self.grow.clone();
        self.map_constituents(
            move |_| Arc::new(move |s: &Seed| wrapper(s, &|s| old(s))),
            |e| e, |f| f, |l| l,
        )
    }

    pub fn filter_seeds(
        self,
        pred: impl Fn(&Seed) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, move |e| e.filter(pred), |f| f, |l| l)
    }

    pub fn wrap_init(
        self,
        wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_init(wrapper), |l| l)
    }

    pub fn wrap_accumulate(
        self,
        wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_accumulate(wrapper), |l| l)
    }

    pub fn wrap_finalize(
        self,
        wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    ) -> Self {
        self.map_constituents(|g| g, |e| e, |f| f.wrap_finalize(wrapper), |l| l)
    }
}

// ── Lift composition ────────────────────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, Nt: 'static, L>
    SeedPipeline<N, Seed, H, R, Nt, L>
{
    /// Compose an outer lift onto the pre-lift chain. `Nt` becomes `Nt2`
    /// — the outer lift's output node type. Inference: `L: Lift<Nt, Nt2>`
    /// picks `Nt2` from the outer lift's impl.
    pub fn apply_pre_lift<L2, Nt2: 'static>(
        self,
        outer: L2,
    ) -> SeedPipeline<N, Seed, H, R, Nt2, ComposedLift<L, L2, Nt>>
    where
        L2: Lift<Nt, Nt2>,
    {
        self.map_constituents(|g| g, |e| e, |f| f, move |l| ComposedLift::compose(l, outer))
    }
}
