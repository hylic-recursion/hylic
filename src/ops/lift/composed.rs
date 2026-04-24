//! ComposedLift — atom of Lift composition. Domain-generic CPS-nested
//! apply. Outer's input = inner's output; both impls must share D.

use crate::domain::Domain;
use super::core::Lift;

// ANCHOR: composed_lift
/// Sequential composition of two lifts. `L1` runs first; `L2`
/// takes `L1`'s outputs as its inputs. The outer lift's `apply`
/// drives this composition.
#[must_use]
pub struct ComposedLift<L1, L2> {
    pub(crate) inner: L1,
    pub(crate) outer: L2,
}
// ANCHOR_END: composed_lift

impl<L1: Clone, L2: Clone> Clone for ComposedLift<L1, L2> {
    fn clone(&self) -> Self {
        ComposedLift { inner: self.inner.clone(), outer: self.outer.clone() }
    }
}

impl<L1, L2> ComposedLift<L1, L2> {
    /// Construct a composed lift from an inner and outer lift.
    /// The outer's inputs must match the inner's outputs.
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

    fn project_entry_node(&self, n: N) -> L2::N2 {
        self.outer.project_entry_node(self.inner.project_entry_node(n))
    }
    fn project_entry_heap(&self, h: H) -> L2::MapH {
        self.outer.project_entry_heap(self.inner.project_entry_heap(h))
    }

    fn apply<T>(
        &self,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <D as Domain<L2::N2>>::Graph<L2::N2>,
            <D as Domain<L2::N2>>::Fold<L2::MapH, L2::MapR>,
        ) -> T,
    ) -> T {
        self.inner.apply(treeish, fold, |t1, f1| {
            self.outer.apply(t1, f1, cont)
        })
    }
}
