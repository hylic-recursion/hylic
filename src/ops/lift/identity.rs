//! IdentityLift — unit of Lift composition. Polymorphic over any
//! domain D and any (N, H, R).

use crate::domain::Domain;
use super::core::Lift;

#[derive(Clone, Copy)]
pub struct IdentityLift;

impl<D, N, H, R> Lift<D, N, H, R> for IdentityLift
where D: Domain<N>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2   = N;
    type MapH = H;
    type MapR = R;

    fn apply<Seed, T>(
        &self,
        grow:    D::Grow<Seed, N>,
        treeish: D::Graph<N, N>,
        fold:    D::Fold<H, R>,
        cont: impl FnOnce(
            D::Grow<Seed, N>,
            D::Graph<N, N>,
            D::Fold<H, R>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        cont(grow, treeish, fold)
    }
}
