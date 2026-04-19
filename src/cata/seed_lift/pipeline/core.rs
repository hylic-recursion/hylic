//! SeedPipeline — base (grow, seeds, fold) + pre-lift chain.
//!
//! `drive` is the sole CPS execution primitive. Bounds bundled via
//! `SharedDomainLift<N, Seed, H, R>`.

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::{Edgy, Treeish};
use crate::ops::{IdentityLift, SharedDomainLift};
use super::super::types::{LiftedHeap, LiftedNode};
use super::super::lift::SeedLift;

pub struct SeedPipeline<N, Seed, H, R, L = IdentityLift> {
    pub(crate) grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
    pub(crate) seeds_from_node: Edgy<N, Seed>,
    pub(crate) fold: Fold<N, H, R>,
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

impl<N: 'static, Seed: 'static, H: 'static, R: 'static>
    SeedPipeline<N, Seed, H, R, IdentityLift>
{
    pub fn new(
        grow: impl Fn(&Seed) -> N + Send + Sync + 'static,
        seeds_from_node: Edgy<N, Seed>,
        fold: &Fold<N, H, R>,
    ) -> Self {
        SeedPipeline {
            grow: Arc::new(grow),
            seeds_from_node,
            fold: fold.clone(),
            pre_lift: IdentityLift,
        }
    }
}

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where L: SharedDomainLift<N, Seed, H, R>,
      N: Clone + Send + Sync + 'static,
      Seed: Clone + Send + Sync + 'static,
      H: Clone + Send + Sync + 'static,
      R: Clone + Send + Sync + 'static,
      L::N2: Clone + Send + Sync + 'static,
      L::Seed2: Clone + Send + Sync + 'static,
      L::MapH: Clone + Send + Sync + 'static,
      L::MapR: Clone + Send + Sync + 'static,
{
    pub fn drive<T>(
        &self,
        entry_seeds: Edgy<(), L::Seed2>,
        entry_heap_fn: impl Fn() -> L::MapH + Send + Sync + 'static,
        cont: impl FnOnce(
            &Fold<LiftedNode<L::Seed2, L::N2>, LiftedHeap<L::MapH, L::MapR>, L::MapR>,
            &Treeish<LiftedNode<L::Seed2, L::N2>>,
        ) -> T,
    ) -> T {
        let base_treeish: Treeish<N> = {
            let g = self.grow.clone();
            self.seeds_from_node.clone().map(move |s: &Seed| g(s))
        };
        self.pre_lift.apply(
            self.grow.clone(),
            self.seeds_from_node.clone(),
            base_treeish,
            self.fold.clone(),
            |grow_lifted, _seeds_lifted, treeish_lifted, fold_lifted| {
                let sl = SeedLift { grow: grow_lifted };
                let lifted_treeish = sl.lift_treeish(treeish_lifted, entry_seeds);
                let lifted_fold = sl.lift_fold(fold_lifted, entry_heap_fn);
                cont(&lifted_fold, &lifted_treeish)
            },
        )
    }
}
