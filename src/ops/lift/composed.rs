//! ComposedLift — atom of Lift composition. CPS-nested apply.

use std::sync::Arc;
use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
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

impl<N, H, R, L1, L2> Lift<N, H, R> for ComposedLift<L1, L2>
where
    N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    L1: Lift<N, H, R>,
    L2: Lift<L1::N2, L1::MapH, L1::MapR>,
{
    type N2   = L2::N2;
    type MapH = L2::MapH;
    type MapR = L2::MapR;

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> Self::N2 + Send + Sync>,
            Treeish<Self::N2>,
            Fold<Self::N2, Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        self.inner.apply::<Seed, _>(grow, treeish, fold, |g1, t1, f1| {
            self.outer.apply::<Seed, _>(g1, t1, f1, cont)
        })
    }
}
