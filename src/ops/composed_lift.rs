//! ComposedLift: bifunctor composition L2 ∘ L1.
//!
//! L1 transforms first (inner), L2 transforms L1's output (outer).
//! Blanket impl — any two Lift impls compose automatically.

use std::marker::PhantomData;
use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use super::lift::Lift;

pub struct ComposedLift<L1, L2, Nmid> {
    pub(crate) inner: L1,
    pub(crate) outer: L2,
    pub(crate) _mid: PhantomData<fn() -> Nmid>,
}

impl<L1: Clone, L2: Clone, Nmid> Clone for ComposedLift<L1, L2, Nmid> {
    fn clone(&self) -> Self {
        ComposedLift { inner: self.inner.clone(), outer: self.outer.clone(), _mid: PhantomData }
    }
}

impl<L1, L2, Nmid> ComposedLift<L1, L2, Nmid> {
    pub fn compose(inner: L1, outer: L2) -> Self {
        ComposedLift { inner, outer, _mid: PhantomData }
    }
}

impl<N, Nmid, N2, L1, L2> Lift<N, N2> for ComposedLift<L1, L2, Nmid>
where
    N: Clone + 'static,
    Nmid: Clone + 'static,
    N2: Clone + 'static,
    L1: Lift<N, Nmid>,
    L2: Lift<Nmid, N2>,
{
    type MapH<H: Clone + 'static, R: Clone + 'static> =
        L2::MapH<L1::MapH<H, R>, L1::MapR<H, R>>;
    type MapR<H: Clone + 'static, R: Clone + 'static> =
        L2::MapR<L1::MapH<H, R>, L1::MapR<H, R>>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2> {
        self.outer.lift_treeish(self.inner.lift_treeish(t))
    }

    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N2, Self::MapH<H, R>, Self::MapR<H, R>> {
        self.outer.lift_fold(self.inner.lift_fold(f))
    }

    fn lift_root(&self, root: &N) -> N2 {
        self.outer.lift_root(&self.inner.lift_root(root))
    }
}
