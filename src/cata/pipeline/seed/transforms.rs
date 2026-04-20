//! Stage-1 sugars — Shared-domain only for now (Phase 5/5).
//! Per-domain variants can be added following the same pattern.
//!
//! - filter_seeds(p)            — narrows the seed set (N/Seed/H/R preserved)
//! - wrap_grow(w)                — wraps grow closure (N/Seed/H/R preserved)
//! - contramap_node(co, contra)  — changes N to N2 via bijection
//! - map_seed(to, from)          — changes Seed to Seed2 via bijection

use std::sync::Arc;
use crate::domain::Shared;
use crate::domain::shared::fold::Fold;
use crate::graph::Edgy;
use super::SeedPipeline;

impl<N, Seed, H, R> SeedPipeline<Shared, N, Seed, H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    pub fn filter_seeds<P>(self, pred: P) -> SeedPipeline<Shared, N, Seed, H, R>
    where P: Fn(&Seed) -> bool + Send + Sync + 'static,
    {
        let pred = Arc::new(pred);
        self.reshape(
            |grow| grow,
            move |seeds: Edgy<N, Seed>| seeds.filter(move |s: &Seed| pred(s)),
            |fold| fold,
        )
    }

    pub fn wrap_grow<W>(self, wrapper: W) -> SeedPipeline<Shared, N, Seed, H, R>
    where W: Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + Send + Sync + 'static,
    {
        let wrapper = Arc::new(wrapper);
        self.reshape(
            move |grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>|
                -> Arc<dyn Fn(&Seed) -> N + Send + Sync>
            {
                let w = wrapper.clone();
                let orig = grow.clone();
                Arc::new(move |s: &Seed| w(s, &|s: &Seed| orig(s)))
            },
            |seeds| seeds,
            |fold| fold,
        )
    }

    pub fn contramap_node<N2, Co, Contra>(
        self,
        co: Co,
        contra: Contra,
    ) -> SeedPipeline<Shared, N2, Seed, H, R>
    where N2: Clone + 'static,
          Co:     Fn(&N) -> N2 + Send + Sync + 'static,
          Contra: Fn(&N2) -> N + Send + Sync + 'static,
    {
        let co = Arc::new(co);
        let contra = Arc::new(contra);
        let co_for_grow = co.clone();
        let contra_for_seeds = contra.clone();
        let contra_for_fold = contra.clone();
        self.reshape(
            move |grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>|
                -> Arc<dyn Fn(&Seed) -> N2 + Send + Sync>
            {
                let co = co_for_grow;
                Arc::new(move |s: &Seed| co(&grow(s)))
            },
            move |seeds: Edgy<N, Seed>| -> Edgy<N2, Seed> {
                let contra = contra_for_seeds;
                seeds.contramap(move |n2: &N2| contra(n2))
            },
            move |fold: Fold<N, H, R>| -> Fold<N2, H, R> {
                let contra = contra_for_fold;
                fold.contramap(move |n2: &N2| contra(n2))
            },
        )
    }

    pub fn map_seed<Seed2, ToNew, FromNew>(
        self,
        to_new: ToNew,
        from_new: FromNew,
    ) -> SeedPipeline<Shared, N, Seed2, H, R>
    where Seed2: Clone + 'static,
          ToNew:   Fn(&Seed) -> Seed2 + Send + Sync + 'static,
          FromNew: Fn(&Seed2) -> Seed + Send + Sync + 'static,
    {
        let to_new = Arc::new(to_new);
        let from_new = Arc::new(from_new);
        let from_for_grow = from_new.clone();
        self.reshape(
            move |grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>|
                -> Arc<dyn Fn(&Seed2) -> N + Send + Sync>
            {
                let from_new = from_for_grow;
                Arc::new(move |s2: &Seed2| grow(&from_new(s2)))
            },
            move |seeds: Edgy<N, Seed>| -> Edgy<N, Seed2> {
                let to_new = to_new;
                seeds.map(move |s: &Seed| to_new(s))
            },
            |fold| fold,
        )
    }
}
