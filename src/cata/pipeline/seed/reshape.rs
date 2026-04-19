//! reshape — the sole Stage-1 primitive. Rewrites all three base
//! slots consistently; every Stage-1 sugar is a one-line wrapper
//! over this.

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::Edgy;
use super::SeedPipeline;

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R> {
    pub fn reshape<N2, Seed2, H2, R2, FGrow, FSeeds, FFold>(
        self,
        reshape_grow:  FGrow,
        reshape_seeds: FSeeds,
        reshape_fold:  FFold,
    ) -> SeedPipeline<N2, Seed2, H2, R2>
    where
        FGrow:  FnOnce(Arc<dyn Fn(&Seed) -> N + Send + Sync>)
                       -> Arc<dyn Fn(&Seed2) -> N2 + Send + Sync>,
        FSeeds: FnOnce(Edgy<N, Seed>) -> Edgy<N2, Seed2>,
        FFold:  FnOnce(Fold<N, H, R>) -> Fold<N2, H2, R2>,
    {
        SeedPipeline {
            grow:            reshape_grow(self.grow),
            seeds_from_node: reshape_seeds(self.seeds_from_node),
            fold:            reshape_fold(self.fold),
        }
    }
}
