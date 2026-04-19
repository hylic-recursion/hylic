//! ComposedLift — CPS-nested apply. L2 ∘ L1, one line of logic.

use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use super::lift::Lift;

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

impl<L1: Lift, L2: Lift> Lift for ComposedLift<L1, L2> {
    type N2<N: Clone + 'static>       = L2::N2<L1::N2<N>>;
    type Seed2<Seed: Clone + 'static> = L2::Seed2<L1::Seed2<Seed>>;
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>
        = L2::MapH<L1::N2<N>, L1::MapH<N, H, R>, L1::MapR<N, H, R>>;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>
        = L2::MapR<L1::N2<N>, L1::MapH<N, H, R>, L1::MapR<N, H, R>>;

    fn apply<N, Seed, H, R, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed2<Seed>) -> Self::N2<N> + Send + Sync>,
            Edgy<Self::N2<N>, Self::Seed2<Seed>>,
            Treeish<Self::N2<N>>,
            Fold<Self::N2<N>, Self::MapH<N, H, R>, Self::MapR<N, H, R>>,
        ) -> T,
    ) -> T
    where N: Clone + 'static, Seed: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        self.inner.apply(grow, seeds, treeish, fold, |g1, s1, t1, f1| {
            self.outer.apply(g1, s1, t1, f1, |g2, s2, t2, f2| {
                cont(g2, s2, t2, f2)
            })
        })
    }

    fn lift_root<N: Clone + 'static>(&self, root: &N) -> Self::N2<N> {
        self.outer.lift_root(&self.inner.lift_root(root))
    }
}
