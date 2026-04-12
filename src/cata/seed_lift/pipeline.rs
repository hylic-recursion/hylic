//! SeedPipeline: the user-facing wrapper. Stores grow, seeds_from_node,
//! and fold decomposed. Fusion happens at point of use.

use std::sync::Arc;
use crate::domain::shared;
use crate::graph::Edgy;

pub struct SeedPipeline<N, Seed, H, R> {
    pub(crate) grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
    pub(crate) seeds_from_node: Edgy<N, Seed>,
    pub(crate) fold: shared::fold::Fold<N, H, R>,
}

impl<N, Seed, H, R> Clone for SeedPipeline<N, Seed, H, R> {
    fn clone(&self) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.clone(),
        }
    }
}

impl<N: 'static, Seed: 'static, H: 'static, R: 'static> SeedPipeline<N, Seed, H, R> {
    pub fn new(
        grow: impl Fn(&Seed) -> N + Send + Sync + 'static,
        seeds_from_node: Edgy<N, Seed>,
        fold: &shared::fold::Fold<N, H, R>,
    ) -> Self {
        SeedPipeline {
            grow: Arc::new(grow),
            seeds_from_node,
            fold: fold.clone(),
        }
    }
}
