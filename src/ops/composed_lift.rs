//! ComposedLift: functor composition L2 ∘ L1.
//!
//! L1 transforms first (inner), L2 transforms L1's output (outer).
//! Implements LiftOps for the composed pair.

use std::marker::PhantomData;
use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use super::lift::LiftOps;
use super::outer_lift::OuterLift;

pub struct ComposedLift<L1, L2, Nmid> {
    pub(crate) inner: L1,
    pub(crate) outer: L2,
    pub(crate) _mid: PhantomData<fn() -> Nmid>,
}

impl<L1: Clone, L2: Clone, Nmid> Clone for ComposedLift<L1, L2, Nmid> {
    fn clone(&self) -> Self {
        ComposedLift {
            inner: self.inner.clone(),
            outer: self.outer.clone(),
            _mid: PhantomData,
        }
    }
}

impl<L1, L2, Nmid> ComposedLift<L1, L2, Nmid> {
    pub fn compose(inner: L1, outer: L2) -> Self {
        ComposedLift { inner, outer, _mid: PhantomData }
    }
}

impl<N, R, Nmid, N2, L1, L2> LiftOps<N, R, N2> for ComposedLift<L1, L2, Nmid>
where
    N: Clone + 'static,
    R: Clone + 'static,
    Nmid: Clone + 'static,
    N2: Clone + 'static,
    L1: LiftOps<N, R, Nmid>,
    L2: OuterLift<L1, N, R, Nmid, N2>,
{
    type LiftedH<H: Clone + 'static> = L2::LiftedH<H>;
    type LiftedR<H: Clone + 'static> = L2::LiftedR<H>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2> {
        self.outer.lift_treeish(self.inner.lift_treeish(t))
    }

    fn lift_fold<H: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N2, Self::LiftedH<H>, Self::LiftedR<H>> {
        self.outer.lift_fold(self.inner.lift_fold(f))
    }

    fn lift_root(&self, root: &N) -> N2 {
        self.outer.lift_root(&self.inner.lift_root(root))
    }
}
