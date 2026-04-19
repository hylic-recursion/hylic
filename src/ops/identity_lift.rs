//! IdentityLift — unit of Lift composition. Polymorphic via impl<…>.

use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use super::lift::Lift;

#[derive(Clone, Copy)]
pub struct IdentityLift;

impl<N, Seed, H, R> Lift<N, Seed, H, R> for IdentityLift
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;
    type Seed2 = Seed;
    type MapH = H;
    type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        cont(grow, seeds, treeish, fold)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}
