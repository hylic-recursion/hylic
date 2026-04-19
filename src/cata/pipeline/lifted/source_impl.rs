//! impl PipelineSource for LiftedPipeline — delegates to the base's
//! PipelineSource impl, then runs the lift chain's apply on the
//! yielded triple.

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use crate::ops::Lift;
use super::LiftedPipeline;
use super::super::source::PipelineSource;

impl<N, Seed, H, R, L> PipelineSource for LiftedPipeline<N, Seed, H, R, L>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      L: Lift<N, H, R>,
      L::N2:   Clone + 'static,
      L::MapH: Clone + 'static,
      L::MapR: Clone + 'static,
{
    type Seed = Seed;
    type N    = L::N2;
    type H    = L::MapH;
    type R    = L::MapR;

    fn with_constructed<T>(
        &self,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed) -> Self::N + Send + Sync>,
            Treeish<Self::N>,
            Fold<Self::N, Self::H, Self::R>,
        ) -> T,
    ) -> T {
        self.base.with_constructed(|grow, treeish, fold| {
            self.pre_lift.apply::<Seed, _>(grow, treeish, fold, cont)
        })
    }
}
