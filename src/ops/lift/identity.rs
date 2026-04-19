//! IdentityLift — unit of Lift composition. Polymorphic in (N, H, R);
//! Seed is method-level.

use std::sync::Arc;
use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use super::core::Lift;

#[derive(Clone, Copy)]
pub struct IdentityLift;

impl<N, H, R> Lift<N, H, R> for IdentityLift
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2   = N;
    type MapH = H;
    type MapR = R;

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
        cont(grow, treeish, fold)
    }
}
