//! R-transform lifts: ZipmapLift (R → (R, Extra)), MapRLift (R ↔ RNew).

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use crate::ops::lift::core::Lift;

// ── ZipmapLift: R → (R, Extra) ──────────────────────

pub struct ZipmapLift<R, Extra> {
    mapper: Arc<dyn Fn(&R) -> Extra + Send + Sync>,
    _m: PhantomData<fn() -> (R, Extra)>,
}

impl<R, Extra> Clone for ZipmapLift<R, Extra> {
    fn clone(&self) -> Self { ZipmapLift { mapper: self.mapper.clone(), _m: PhantomData } }
}

pub fn zipmap_lift<R, Extra, M>(mapper: M) -> ZipmapLift<R, Extra>
where M: Fn(&R) -> Extra + Send + Sync + 'static,
{
    ZipmapLift { mapper: Arc::new(mapper), _m: PhantomData }
}

impl<N, H, R, Extra> Lift<N, H, R> for ZipmapLift<R, Extra>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
      Extra: Clone + 'static,
{
    type N2 = N;  type MapH = H;  type MapR = (R, Extra);

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Treeish<N>,
            Fold<N, H, (R, Extra)>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        let m = self.mapper.clone();
        let zipped = fold.zipmap(move |r: &R| m(r));
        cont(grow, treeish, zipped)
    }
}

// ── MapRLift: R ↔ RNew (bijection) ──────────────────

pub struct MapRLift<R, RNew> {
    forward:  Arc<dyn Fn(&R) -> RNew + Send + Sync>,
    backward: Arc<dyn Fn(&RNew) -> R + Send + Sync>,
    _m: PhantomData<fn() -> (R, RNew)>,
}

impl<R, RNew> Clone for MapRLift<R, RNew> {
    fn clone(&self) -> Self {
        MapRLift { forward: self.forward.clone(), backward: self.backward.clone(), _m: PhantomData }
    }
}

pub fn map_r_lift<R, RNew, Fwd, Bwd>(forward: Fwd, backward: Bwd) -> MapRLift<R, RNew>
where Fwd: Fn(&R) -> RNew + Send + Sync + 'static,
      Bwd: Fn(&RNew) -> R + Send + Sync + 'static,
{
    MapRLift {
        forward:  Arc::new(forward),
        backward: Arc::new(backward),
        _m: PhantomData,
    }
}

impl<N, H, R, RNew> Lift<N, H, R> for MapRLift<R, RNew>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
      RNew: Clone + 'static,
{
    type N2 = N;  type MapH = H;  type MapR = RNew;

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Treeish<N>,
            Fold<N, H, RNew>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        let fwd = self.forward.clone();
        let bwd = self.backward.clone();
        let mapped = fold.map(move |r: &R| fwd(r), move |r: &RNew| bwd(r));
        cont(grow, treeish, mapped)
    }
}
