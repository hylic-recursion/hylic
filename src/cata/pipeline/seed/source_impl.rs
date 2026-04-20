//! impl PipelineSource for SeedPipeline, and the .lift() transition.
//!
//! `with_constructed` fuses `grow` and `seeds_from_node` into a
//! treeish over N at yield-time; the raw parts are handed back
//! alongside. Domain-generic via the D parameter.

use crate::domain::Domain;
use crate::ops::{IdentityLift, ShapeCapable};
use super::SeedPipeline;
use super::super::source::PipelineSource;
use super::super::lifted::LiftedPipeline;

impl<D, N, Seed, H, R> PipelineSource for SeedPipeline<D, N, Seed, H, R>
where D: ShapeCapable<N>,
      N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      <D as Domain<N>>::Grow<Seed, N>: Clone,
      <D as Domain<N>>::Graph<Seed>:   Clone,
      <D as Domain<N>>::Fold<H, R>:    Clone,
{
    type Domain = D;
    type Seed = Seed;
    type N    = N;
    type H    = H;
    type R    = R;

    fn with_constructed<T>(
        &self,
        cont: impl FnOnce(
            <D as Domain<N>>::Grow<Seed, N>,
            <D as Domain<N>>::Graph<N>,
            <D as Domain<N>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        let treeish = D::fuse_grow_with_seeds::<Seed>(
            self.grow.clone(),
            self.seeds_from_node.clone(),
        );
        cont(self.grow.clone(), treeish, self.fold.clone())
    }
}

impl<D, N, Seed, H, R> SeedPipeline<D, N, Seed, H, R>
where D: Domain<N>,
      N: 'static, Seed: 'static, H: 'static, R: 'static,
{
    /// Transition to Stage 2 with an IdentityLift. The pipeline's
    /// base slots move into the LiftedPipeline's `base` field.
    pub fn lift(self) -> LiftedPipeline<Self, IdentityLift> {
        LiftedPipeline::new(self, IdentityLift)
    }
}
