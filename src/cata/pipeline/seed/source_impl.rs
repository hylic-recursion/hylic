//! impl PipelineSource for SeedPipeline, and the .lift() transition.

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use crate::ops::IdentityLift;
use super::SeedPipeline;
use super::super::source::PipelineSource;
use super::super::lifted::LiftedPipeline;

impl<N, Seed, H, R> PipelineSource for SeedPipeline<N, Seed, H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type Seed = Seed;
    type N    = N;
    type H    = H;
    type R    = R;

    fn with_constructed<T>(
        &self,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed) -> Self::N + Send + Sync>,
            Treeish<Self::N>,
            Fold<Self::N, Self::H, Self::R>,
        ) -> T,
    ) -> T {
        let treeish: Treeish<N> = {
            let g = self.grow.clone();
            self.seeds_from_node.clone().map(move |s: &Seed| g(s))
        };
        cont(self.grow.clone(), treeish, self.fold.clone())
    }
}

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R> {
    /// Transition to Stage 2 with an IdentityLift. The pipeline's
    /// base slots move into the LiftedPipeline's `base` field.
    pub fn lift(self) -> LiftedPipeline<N, Seed, H, R, IdentityLift> {
        LiftedPipeline::new(self, IdentityLift)
    }
}
