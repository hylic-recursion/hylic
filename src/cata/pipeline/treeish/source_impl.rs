//! impl PipelineSource for TreeishPipeline + .lift() transition.
//!
//! `Self::Seed = ()` — no Seed-to-Node resolution step. The
//! Arc<Fn(&()) -> N> supplied to `with_constructed` is a panic
//! closure; it's unreachable under every legitimate code path
//! (`run_from_node` ignores grow; `run` / `run_from_slice` cannot
//! bind Seed=() in a way that a user would pass meaningful entry
//! seeds through).

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use crate::ops::IdentityLift;
use super::TreeishPipeline;
use super::super::source::PipelineSource;
use super::super::lifted::LiftedPipeline;

impl<N, H, R> PipelineSource for TreeishPipeline<N, H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type Seed = ();
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
        let grow: Arc<dyn Fn(&()) -> N + Send + Sync> = Arc::new(|_: &()| {
            unreachable!("TreeishPipeline has no Seed→N step; \
                          use run_from_node, not run or run_from_slice")
        });
        cont(grow, self.treeish.clone(), self.fold.clone())
    }
}

impl<N, H, R> TreeishPipeline<N, H, R> {
    /// Transition to Stage 2 with an IdentityLift.
    pub fn lift(self) -> LiftedPipeline<Self, IdentityLift> {
        LiftedPipeline::new(self, IdentityLift)
    }
}
