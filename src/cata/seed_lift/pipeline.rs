//! SeedPipeline: the user-facing wrapper. Stores grow, seeds_from_node,
//! fold, and a pre-lift L (default IdentityLift). Fusion happens at
//! point of use — compose treeish from parts, apply pre-lift, then
//! apply SeedLift.

use std::sync::Arc;
use crate::domain::shared;
use crate::graph::Edgy;
use crate::ops::IdentityLift;

pub struct SeedPipeline<N, Seed, H, R, L = IdentityLift> {
    pub(crate) grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
    pub(crate) seeds_from_node: Edgy<N, Seed>,
    pub(crate) fold: shared::fold::Fold<N, H, R>,
    pub(crate) pre_lift: L,
}

impl<N, Seed, H, R, L: Clone> Clone for SeedPipeline<N, Seed, H, R, L> {
    fn clone(&self) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.clone(),
            pre_lift: self.pre_lift.clone(),
        }
    }
}

impl<N: 'static, Seed: 'static, H: 'static, R: 'static> SeedPipeline<N, Seed, H, R, IdentityLift> {
    pub fn new(
        grow: impl Fn(&Seed) -> N + Send + Sync + 'static,
        seeds_from_node: Edgy<N, Seed>,
        fold: &shared::fold::Fold<N, H, R>,
    ) -> Self {
        SeedPipeline {
            grow: Arc::new(grow),
            seeds_from_node,
            fold: fold.clone(),
            pre_lift: IdentityLift,
        }
    }
}
