//! impl PipelineSource for LiftedPipeline<Base, L> — delegates to
//! the base's PipelineSource impl, then runs the lift chain's
//! apply on the yielded triple.

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use crate::ops::Lift;
use super::LiftedPipeline;
use super::super::source::PipelineSource;

impl<Base, L> PipelineSource for LiftedPipeline<Base, L>
where Base: PipelineSource,
      L: Lift<Base::N, Base::H, Base::R>,
      L::N2:   Clone + 'static,
      L::MapH: Clone + 'static,
      L::MapR: Clone + 'static,
{
    type Seed = Base::Seed;
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
            self.pre_lift.apply::<Base::Seed, _>(grow, treeish, fold, cont)
        })
    }
}
