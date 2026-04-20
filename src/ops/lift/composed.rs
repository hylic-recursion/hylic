// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! ComposedLift — atom of Lift composition. Domain-generic CPS-nested
//! apply. Outer's input = inner's output; both impls must share D.

use crate::domain::Domain;
use super::core::Lift;

pub struct ComposedLift<L1, L2> {
    pub(crate) inner: L1,
    pub(crate) outer: L2,
}

impl<L1: Clone, L2: Clone> Clone for ComposedLift<L1, L2> {
    fn clone(&self) -> Self {
        ComposedLift { inner: self.inner.clone(), outer: self.outer.clone() }
    }
}

impl<L1, L2> ComposedLift<L1, L2> {
    pub fn compose(inner: L1, outer: L2) -> Self {
        ComposedLift { inner, outer }
    }
}

impl<D, N, H, R, L1, L2> Lift<D, N, H, R> for ComposedLift<L1, L2>
where
    D: Domain<N> + Domain<L1::N2> + Domain<L2::N2>,
    N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    L1: Lift<D, N, H, R>,
    L2: Lift<D, L1::N2, L1::MapH, L1::MapR>,
{
    type N2   = L2::N2;
    type MapH = L2::MapH;
    type MapR = L2::MapR;

    fn apply<Seed, T>(
        &self,
        grow:    <D as Domain<N>>::Grow<Seed, N>,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <D as Domain<L2::N2>>::Grow<Seed, L2::N2>,
            <D as Domain<L2::N2>>::Graph<L2::N2>,
            <D as Domain<L2::N2>>::Fold<L2::MapH, L2::MapR>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        self.inner.apply::<Seed, _>(grow, treeish, fold, |g1, t1, f1| {
            self.outer.apply::<Seed, _>(g1, t1, f1, cont)
        })
    }
}
