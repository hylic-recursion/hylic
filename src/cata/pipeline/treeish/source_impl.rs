//! impl PipelineSource for TreeishPipeline + .lift() transition.
//!
//! `Self::Seed = ()` — no Seed-to-Node resolution step. The
//! `Grow<(), N>` supplied to `with_constructed` is an unreachable
//! closure; `run_from_node` ignores it and `run` / `run_from_slice`
//! cannot meaningfully be called on a `Self::Seed = ()` pipeline.

use crate::domain::Domain;
use crate::ops::IdentityLift;
use super::TreeishPipeline;
use super::super::source::PipelineSource;
use super::super::lifted::LiftedPipeline;

impl<D, N, H, R> PipelineSource for TreeishPipeline<D, N, H, R>
where D: Domain<N>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
      <D as Domain<N>>::Graph<N>:   Clone,
      <D as Domain<N>>::Fold<H, R>: Clone,
{
    type Domain = D;
    type Seed = ();
    type N    = N;
    type H    = H;
    type R    = R;

    fn with_constructed<T>(
        &self,
        cont: impl FnOnce(
            <D as Domain<N>>::Grow<(), N>,
            <D as Domain<N>>::Graph<N>,
            <D as Domain<N>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        let grow = D::make_grow::<(), N>(|_: &()| {
            unreachable!("TreeishPipeline has no Seed→N step; \
                          use run_from_node, not run or run_from_slice")
        });
        cont(grow, self.treeish.clone(), self.fold.clone())
    }
}

impl<D, N, H, R> TreeishPipeline<D, N, H, R>
where D: Domain<N>,
      N: 'static, H: 'static, R: 'static,
{
    /// Transition to Stage 2 with an IdentityLift.
    pub fn lift(self) -> LiftedPipeline<Self, IdentityLift> {
        LiftedPipeline::new(self, IdentityLift)
    }
}
