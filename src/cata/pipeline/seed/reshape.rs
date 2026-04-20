//! reshape — the sole Stage-1 primitive. Rewrites all three base
//! slots consistently; every Stage-1 sugar is a one-line wrapper
//! over this. Domain-generic.

use crate::domain::Domain;
use super::SeedPipeline;

impl<D, N, Seed, H, R> SeedPipeline<D, N, Seed, H, R>
where D: Domain<N>,
      N: 'static, Seed: 'static, H: 'static, R: 'static,
{
    pub fn reshape<N2, Seed2, H2, R2, FGrow, FSeeds, FFold>(
        self,
        reshape_grow:  FGrow,
        reshape_seeds: FSeeds,
        reshape_fold:  FFold,
    ) -> SeedPipeline<D, N2, Seed2, H2, R2>
    where
        D: Domain<N2>,
        N2: 'static, Seed2: 'static, H2: 'static, R2: 'static,
        FGrow:  FnOnce(<D as Domain<N>>::Grow<Seed, N>)
                       -> <D as Domain<N2>>::Grow<Seed2, N2>,
        FSeeds: FnOnce(<D as Domain<N>>::Graph<Seed>)
                       -> <D as Domain<N2>>::Graph<Seed2>,
        FFold:  FnOnce(<D as Domain<N>>::Fold<H, R>)
                       -> <D as Domain<N2>>::Fold<H2, R2>,
    {
        SeedPipeline {
            grow:            reshape_grow(self.grow),
            seeds_from_node: reshape_seeds(self.seeds_from_node),
            fold:            reshape_fold(self.fold),
        }
    }
}
