//! IdentityLift — the unit element of Lift composition. Passes all
//! four components through unchanged.

use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use super::lift::Lift;

#[derive(Clone, Copy)]
pub struct IdentityLift;

impl Lift for IdentityLift {
    type N2<N: Clone + 'static> = N;
    type Seed2<Seed: Clone + 'static> = Seed;
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static> = H;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static> = R;

    fn apply<N, Seed, H, R, T>(
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
    ) -> T
    where N: Clone + 'static, Seed: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        cont(grow, seeds, treeish, fold)
    }

    fn lift_root<N: Clone + 'static>(&self, root: &N) -> N { root.clone() }
}
