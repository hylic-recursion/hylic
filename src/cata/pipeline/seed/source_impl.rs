// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! SeedPipeline impls both source traits:
//! - `TreeishSource`: fuses (grow, seeds_from_node) into a treeish
//!   over N at yield time; the caller receives the 2-slot pair.
//! - `SeedSource`: extends with the 3-slot yield that includes the
//!   pipeline's stored `grow` closure, enabling SeedLift composition.

use crate::domain::Domain;
use crate::ops::{IdentityLift, ShapeCapable};
use super::SeedPipeline;
use super::super::source::{TreeishSource, SeedSource};
use super::super::lifted::LiftedPipeline;

impl<D, N, Seed, H, R> TreeishSource for SeedPipeline<D, N, Seed, H, R>
where D: ShapeCapable<N>,
      N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      <D as Domain<N>>::Grow<Seed, N>: Clone,
      <D as Domain<N>>::Graph<Seed>:   Clone,
      <D as Domain<N>>::Fold<H, R>:    Clone,
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
        let treeish = D::fuse_grow_with_seeds::<Seed>(
            self.grow.clone(),
            self.seeds_from_node.clone(),
        );
        cont(treeish, self.fold.clone())
    }
}

impl<D, N, Seed, H, R> SeedSource for SeedPipeline<D, N, Seed, H, R>
where D: ShapeCapable<N>,
      N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      <D as Domain<N>>::Grow<Seed, N>: Clone,
      <D as Domain<N>>::Graph<Seed>:   Clone,
      <D as Domain<N>>::Fold<H, R>:    Clone,
{
    type Seed = Seed;

    fn with_seeded<T>(
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
    /// Transition to Stage 2 with an IdentityLift.
    pub fn lift(self) -> LiftedPipeline<Self, IdentityLift> {
        LiftedPipeline::new(self, IdentityLift)
    }
}
