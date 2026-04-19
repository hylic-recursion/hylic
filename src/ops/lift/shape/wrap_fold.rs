//! Fold-phase wrap lifts: WrapInit, WrapAccumulate, WrapFinalize.
//! All N-preserving, all H/R-preserving, pass grow and treeish through.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use crate::ops::lift::core::Lift;

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

impl<N, H, R> Lift<N, H, R> for WrapInitLift<N, H>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type MapH = H;  type MapR = R;

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        let w = self.wrapper.clone();
        let wrapped = fold.wrap_init(move |n: &N, orig: &dyn Fn(&N) -> H| w(n, orig));
        cont(grow, treeish, wrapped)
    }
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

impl<N, H, R> Lift<N, H, R> for WrapAccumulateLift<H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type MapH = H;  type MapR = R;

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        let w = self.wrapper.clone();
        let wrapped = fold.wrap_accumulate(move |h: &mut H, r: &R, orig: &dyn Fn(&mut H, &R)| w(h, r, orig));
        cont(grow, treeish, wrapped)
    }
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

impl<N, H, R> Lift<N, H, R> for WrapFinalizeLift<H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type MapH = H;  type MapR = R;

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        let w = self.wrapper.clone();
        let wrapped = fold.wrap_finalize(move |h: &H, orig: &dyn Fn(&H) -> R| w(h, orig));
        cont(grow, treeish, wrapped)
    }
}
