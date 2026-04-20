// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! TreeishPipeline impls `TreeishSource` only. It is Seed-agnostic
//! by construction and intentionally does NOT provide `SeedSource`
//! — calling `.run(exec, entry_seeds, entry_heap)` on it is a
//! compile-time error rather than a runtime panic.

use crate::domain::Domain;
use crate::ops::IdentityLift;
use super::TreeishPipeline;
use super::super::source::TreeishSource;
use super::super::lifted::LiftedPipeline;

impl<D, N, H, R> TreeishSource for TreeishPipeline<D, N, H, R>
where D: Domain<N>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
      <D as Domain<N>>::Graph<N>:   Clone,
      <D as Domain<N>>::Fold<H, R>: Clone,
{
    type Domain = D;
    type N = N;
    type H = H;
    type R = R;

    fn with_treeish<T>(
        &self,
        cont: impl FnOnce(
            <D as Domain<N>>::Graph<N>,
            <D as Domain<N>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        cont(self.treeish.clone(), self.fold.clone())
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
