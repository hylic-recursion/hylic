//! IdentityLift — unit of Lift composition. Polymorphic over any
//! domain D and any (N, H, R). N-preserving, trivially.

use crate::domain::Domain;
use super::core::Lift;

#[derive(Clone, Copy)]
#[must_use = "a Lift on its own performs no work; compose or apply it"]
// ANCHOR: identity_lift
/// The pass-through lift — leaves `(grow, treeish, fold)` unchanged
/// across all three axes. Used as the unit of lift composition.
pub struct IdentityLift;
// ANCHOR_END: identity_lift

impl<D, N, H, R> Lift<D, N, H, R> for IdentityLift
where D: Domain<N>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2   = N;
    type MapH = H;
    type MapR = R;

    fn apply<Seed, T>(
        &self,
        grow:    <D as Domain<N>>::Grow<Seed, N>,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <D as Domain<N>>::Grow<Seed, N>,
            <D as Domain<N>>::Graph<N>,
            <D as Domain<N>>::Fold<H, R>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        cont(grow, treeish, fold)
    }
}
