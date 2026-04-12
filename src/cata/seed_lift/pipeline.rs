//! SeedPipeline: the user-facing wrapper. Stores grow, seeds_from_node,
//! fold, and a pre-lift L (default IdentityLift) with output node type
//! Nt (default N).

use std::sync::Arc;
use std::marker::PhantomData;
use crate::domain::shared;
use crate::graph::Edgy;
use crate::ops::IdentityLift;

pub struct SeedPipeline<N, Seed, H, R, Nt = N, L = IdentityLift> {
    pub(crate) grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
    pub(crate) seeds_from_node: Edgy<N, Seed>,
    pub(crate) fold: shared::fold::Fold<N, H, R>,
    pub(crate) pre_lift: L,
    pub(crate) _nt: PhantomData<fn() -> Nt>,
}

impl<N, Seed, H, R, Nt, L: Clone> Clone for SeedPipeline<N, Seed, H, R, Nt, L> {
    fn clone(&self) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.clone(),
            pre_lift: self.pre_lift.clone(),
            _nt: PhantomData,
        }
    }
}

impl<N: 'static, Seed: 'static, H: 'static, R: 'static> SeedPipeline<N, Seed, H, R, N, IdentityLift> {
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
            _nt: PhantomData,
        }
    }
}
