//! R-transform lifts: zipmap, map_r.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use crate::ops::lift::Lift;

// ── ZipmapLift: R → (R, Extra) ──────────────────────

pub struct ZipmapLift<R, Extra, M> {
    mapper: Arc<M>,
    _m: PhantomData<fn() -> (R, Extra)>,
}

impl<R, Extra, M> Clone for ZipmapLift<R, Extra, M> {
    fn clone(&self) -> Self { ZipmapLift { mapper: self.mapper.clone(), _m: PhantomData } }
}

pub fn zipmap_lift<R, Extra, M>(mapper: M) -> ZipmapLift<R, Extra, M>
where M: Fn(&R) -> Extra + Send + Sync + 'static,
{
    ZipmapLift { mapper: Arc::new(mapper), _m: PhantomData }
}

impl<N, Seed, H, R, Extra, M> Lift<N, Seed, H, R> for ZipmapLift<R, Extra, M>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      Extra: Clone + 'static,
      M: Fn(&R) -> Extra + Send + Sync + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = (R, Extra);

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, (R, Extra)>,
        ) -> T,
    ) -> T {
        let m = self.mapper.clone();
        let zipped = fold.zipmap(move |r: &R| m(r));
        cont(grow, seeds, treeish, zipped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── MapRLift: R → RNew (bijection) ──────────────────

pub struct MapRLift<R, RNew, Fwd, Bwd> {
    forward: Arc<Fwd>,
    backward: Arc<Bwd>,
    _m: PhantomData<fn() -> (R, RNew)>,
}

impl<R, RNew, Fwd, Bwd> Clone for MapRLift<R, RNew, Fwd, Bwd> {
    fn clone(&self) -> Self {
        MapRLift { forward: self.forward.clone(), backward: self.backward.clone(), _m: PhantomData }
    }
}

pub fn map_r_lift<R, RNew, Fwd, Bwd>(forward: Fwd, backward: Bwd) -> MapRLift<R, RNew, Fwd, Bwd>
where Fwd: Fn(&R) -> RNew + Send + Sync + 'static,
      Bwd: Fn(&RNew) -> R + Send + Sync + 'static,
{
    MapRLift { forward: Arc::new(forward), backward: Arc::new(backward), _m: PhantomData }
}

impl<N, Seed, H, R, RNew, Fwd, Bwd> Lift<N, Seed, H, R> for MapRLift<R, RNew, Fwd, Bwd>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      RNew: Clone + 'static,
      Fwd: Fn(&R) -> RNew + Send + Sync + 'static,
      Bwd: Fn(&RNew) -> R + Send + Sync + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = RNew;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, RNew>,
        ) -> T,
    ) -> T {
        let fwd = self.forward.clone();
        let bwd = self.backward.clone();
        let mapped = fold.map(move |r: &R| fwd(r), move |r: &RNew| bwd(r));
        cont(grow, seeds, treeish, mapped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}
