//! Fold-phase wrap lifts: wrap_init, wrap_accumulate, wrap_finalize,
//! wrap_grow. Closures erased behind Arc<dyn Fn + Send + Sync>.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use crate::ops::lift::Lift;

// ── WrapInitLift ────────────────────────────────────

pub struct WrapInitLift<N, H> {
    wrapper: Arc<dyn Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync>,
    _m: PhantomData<fn() -> (N, H)>,
}

impl<N, H> Clone for WrapInitLift<N, H> {
    fn clone(&self) -> Self { WrapInitLift { wrapper: self.wrapper.clone(), _m: PhantomData } }
}

pub fn wrap_init_lift<N, H, W>(wrapper: W) -> WrapInitLift<N, H>
where W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
{
    WrapInitLift { wrapper: Arc::new(wrapper), _m: PhantomData }
}

impl<N, Seed, H, R> Lift<N, Seed, H, R> for WrapInitLift<N, H>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

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
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let w = self.wrapper.clone();
        let wrapped = fold.wrap_init(move |n: &N, orig: &dyn Fn(&N) -> H| w(n, orig));
        cont(grow, seeds, treeish, wrapped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── WrapAccumulateLift ───────────────────────────────

pub struct WrapAccumulateLift<H, R> {
    wrapper: Arc<dyn Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync>,
    _m: PhantomData<fn() -> (H, R)>,
}

impl<H, R> Clone for WrapAccumulateLift<H, R> {
    fn clone(&self) -> Self { WrapAccumulateLift { wrapper: self.wrapper.clone(), _m: PhantomData } }
}

pub fn wrap_accumulate_lift<H, R, W>(wrapper: W) -> WrapAccumulateLift<H, R>
where W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
{
    WrapAccumulateLift { wrapper: Arc::new(wrapper), _m: PhantomData }
}

impl<N, Seed, H, R> Lift<N, Seed, H, R> for WrapAccumulateLift<H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

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
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let w = self.wrapper.clone();
        let wrapped = fold.wrap_accumulate(move |h: &mut H, r: &R, orig: &dyn Fn(&mut H, &R)| w(h, r, orig));
        cont(grow, seeds, treeish, wrapped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── WrapFinalizeLift ─────────────────────────────────

pub struct WrapFinalizeLift<H, R> {
    wrapper: Arc<dyn Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync>,
    _m: PhantomData<fn() -> (H, R)>,
}

impl<H, R> Clone for WrapFinalizeLift<H, R> {
    fn clone(&self) -> Self { WrapFinalizeLift { wrapper: self.wrapper.clone(), _m: PhantomData } }
}

pub fn wrap_finalize_lift<H, R, W>(wrapper: W) -> WrapFinalizeLift<H, R>
where W: Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
{
    WrapFinalizeLift { wrapper: Arc::new(wrapper), _m: PhantomData }
}

impl<N, Seed, H, R> Lift<N, Seed, H, R> for WrapFinalizeLift<H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

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
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let w = self.wrapper.clone();
        let wrapped = fold.wrap_finalize(move |h: &H, orig: &dyn Fn(&H) -> R| w(h, orig));
        cont(grow, seeds, treeish, wrapped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── WrapGrowLift ─────────────────────────────────────

pub struct WrapGrowLift<N, Seed> {
    wrapper: Arc<dyn Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + Send + Sync>,
    _m: PhantomData<fn() -> (N, Seed)>,
}

impl<N, Seed> Clone for WrapGrowLift<N, Seed> {
    fn clone(&self) -> Self { WrapGrowLift { wrapper: self.wrapper.clone(), _m: PhantomData } }
}

pub fn wrap_grow_lift<N, Seed, W>(wrapper: W) -> WrapGrowLift<N, Seed>
where W: Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + Send + Sync + 'static,
{
    WrapGrowLift { wrapper: Arc::new(wrapper), _m: PhantomData }
}

impl<N, Seed, H, R> Lift<N, Seed, H, R> for WrapGrowLift<N, Seed>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        _treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let w = self.wrapper.clone();
        let old_grow = grow.clone();
        let new_grow: Arc<dyn Fn(&Seed) -> N + Send + Sync> =
            Arc::new(move |s: &Seed| w(s, &|s| old_grow(s)));
        let new_treeish: Treeish<N> = {
            let g = new_grow.clone();
            seeds.clone().map(move |s: &Seed| g(s))
        };
        cont(new_grow, seeds, new_treeish, fold)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}
