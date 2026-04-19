//! Type-changing constituent lifts: contramap_node, map_seed.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use crate::ops::lift::Lift;

// ── ContramapNodeLift: N → N2 bijection ─────────────

pub struct ContramapNodeLift<N, N2, Co, Contra> {
    co: Arc<Co>,
    contra: Arc<Contra>,
    _m: PhantomData<fn() -> (N, N2)>,
}

impl<N, N2, Co, Contra> Clone for ContramapNodeLift<N, N2, Co, Contra> {
    fn clone(&self) -> Self {
        ContramapNodeLift { co: self.co.clone(), contra: self.contra.clone(), _m: PhantomData }
    }
}

pub fn contramap_node_lift<N, N2, Co, Contra>(co: Co, contra: Contra)
    -> ContramapNodeLift<N, N2, Co, Contra>
where Co: Fn(&N) -> N2 + Send + Sync + 'static,
      Contra: Fn(&N2) -> N + Send + Sync + 'static,
{
    ContramapNodeLift { co: Arc::new(co), contra: Arc::new(contra), _m: PhantomData }
}

impl<N, Seed, H, R, N2, Co, Contra> Lift<N, Seed, H, R>
    for ContramapNodeLift<N, N2, Co, Contra>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      N2: Clone + 'static,
      Co: Fn(&N) -> N2 + Send + Sync + 'static,
      Contra: Fn(&N2) -> N + Send + Sync + 'static,
{
    type N2 = N2;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        _treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N2 + Send + Sync>,
            Edgy<N2, Seed>,
            Treeish<N2>,
            Fold<N2, H, R>,
        ) -> T,
    ) -> T {
        let co = self.co.clone();
        let contra = self.contra.clone();
        // grow: Seed → N. new_grow: Seed → N2 via co.
        let new_grow: Arc<dyn Fn(&Seed) -> N2 + Send + Sync> = {
            let co = co.clone();
            Arc::new(move |s: &Seed| co(&grow(s)))
        };
        // seeds: Edgy<N, Seed> → Edgy<N2, Seed> via contramap.
        let new_seeds = {
            let contra = contra.clone();
            seeds.contramap(move |n2: &N2| contra(n2))
        };
        // treeish rebuilt from new_grow and new_seeds.
        let new_treeish: Treeish<N2> = {
            let g = new_grow.clone();
            new_seeds.clone().map(move |s: &Seed| g(s))
        };
        // fold: contramap to see N via &N2.
        let new_fold = fold.contramap(move |n2: &N2| contra(n2));
        cont(new_grow, new_seeds, new_treeish, new_fold)
    }

    fn lift_root(&self, root: &N) -> N2 { (self.co)(root) }
}

// ── MapSeedLift: Seed → Seed2 bijection ─────────────

pub struct MapSeedLift<Seed, Seed2, ToNew, FromNew> {
    to_new: Arc<ToNew>,
    from_new: Arc<FromNew>,
    _m: PhantomData<fn() -> (Seed, Seed2)>,
}

impl<Seed, Seed2, ToNew, FromNew> Clone for MapSeedLift<Seed, Seed2, ToNew, FromNew> {
    fn clone(&self) -> Self {
        MapSeedLift { to_new: self.to_new.clone(), from_new: self.from_new.clone(), _m: PhantomData }
    }
}

pub fn map_seed_lift<Seed, Seed2, ToNew, FromNew>(to_new: ToNew, from_new: FromNew)
    -> MapSeedLift<Seed, Seed2, ToNew, FromNew>
where ToNew: Fn(&Seed) -> Seed2 + Send + Sync + 'static,
      FromNew: Fn(&Seed2) -> Seed + Send + Sync + 'static,
{
    MapSeedLift { to_new: Arc::new(to_new), from_new: Arc::new(from_new), _m: PhantomData }
}

impl<N, Seed, H, R, Seed2, ToNew, FromNew> Lift<N, Seed, H, R>
    for MapSeedLift<Seed, Seed2, ToNew, FromNew>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      Seed2: Clone + 'static,
      ToNew: Fn(&Seed) -> Seed2 + Send + Sync + 'static,
      FromNew: Fn(&Seed2) -> Seed + Send + Sync + 'static,
{
    type N2 = N;  type Seed2 = Seed2;  type MapH = H;  type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        _treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed2) -> N + Send + Sync>,
            Edgy<N, Seed2>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let from_new = self.from_new.clone();
        let to_new = self.to_new.clone();
        let new_grow: Arc<dyn Fn(&Seed2) -> N + Send + Sync> = {
            let from_new = from_new.clone();
            Arc::new(move |s: &Seed2| grow(&from_new(s)))
        };
        let new_seeds: Edgy<N, Seed2> = seeds.map(move |s: &Seed| to_new(s));
        let new_treeish: Treeish<N> = {
            let g = new_grow.clone();
            new_seeds.clone().map(move |s: &Seed2| g(s))
        };
        cont(new_grow, new_seeds, new_treeish, fold)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}
