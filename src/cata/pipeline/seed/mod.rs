//! SeedPipeline — Stage 1 of the Phase-3 typestate. Holds the three
//! base slots (grow, seeds_from_node, fold) in the domain D's
//! native storage. Sole primitive: reshape.

use crate::domain::Domain;

pub mod reshape;
pub mod transforms;
pub mod transforms_local;
pub mod source_impl;

pub struct SeedPipeline<D, N, Seed, H, R>
where D: Domain<N>,
      N: 'static, Seed: 'static, H: 'static, R: 'static,
{
    pub(crate) grow:            <D as Domain<N>>::Grow<Seed, N>,
    pub(crate) seeds_from_node: <D as Domain<N>>::Graph<Seed>,
    pub(crate) fold:            <D as Domain<N>>::Fold<H, R>,
}

impl<D, N, Seed, H, R> Clone for SeedPipeline<D, N, Seed, H, R>
where D: Domain<N>,
      N: 'static, Seed: 'static, H: 'static, R: 'static,
      <D as Domain<N>>::Grow<Seed, N>: Clone,
      <D as Domain<N>>::Graph<Seed>:   Clone,
      <D as Domain<N>>::Fold<H, R>:    Clone,
{
    fn clone(&self) -> Self {
        SeedPipeline {
            grow:            self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold:            self.fold.clone(),
        }
    }
}

impl<D, N, Seed, H, R> SeedPipeline<D, N, Seed, H, R>
where D: Domain<N>,
      N: 'static, Seed: 'static, H: 'static, R: 'static,
{
    pub fn new_domain(
        grow:            <D as Domain<N>>::Grow<Seed, N>,
        seeds_from_node: <D as Domain<N>>::Graph<Seed>,
        fold:            <D as Domain<N>>::Fold<H, R>,
    ) -> Self {
        SeedPipeline { grow, seeds_from_node, fold }
    }
}

// ── Shared convenience constructor ─────────────────────

impl<N, Seed, H, R> SeedPipeline<crate::domain::Shared, N, Seed, H, R>
where N: 'static, Seed: 'static, H: 'static, R: 'static,
{
    /// Shared-specific constructor that takes a plain Fn closure for
    /// grow and an `Edgy<N, Seed>` for seeds_from_node. Mirrors the
    /// pre-5/5 API for Shared users.
    pub fn new(
        grow: impl Fn(&Seed) -> N + Send + Sync + 'static,
        seeds_from_node: crate::graph::Edgy<N, Seed>,
        fold: &crate::domain::shared::fold::Fold<N, H, R>,
    ) -> Self {
        SeedPipeline {
            grow: std::sync::Arc::new(grow),
            seeds_from_node,
            fold: fold.clone(),
        }
    }
}
