//! SeedPipeline core — struct + `drive` (sole CPS execution primitive).
//!
//! The pipeline lives in the Shared domain; bound discipline is
//! bundled into `SharedDomainLift`. All transformations (every sugar
//! method) go through `apply_pre_lift` in `transforms.rs`.

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::{Edgy, Treeish};
use crate::ops::{IdentityLift, Lift as _, SharedDomainLift};
use super::super::types::{LiftedHeap, LiftedNode};
use super::super::lift::SeedLift;

// ── Struct ──────────────────────────────────────────

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

// ── Constructor ─────────────────────────────────────

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

// ── drive: CPS execution primitive ──────────────────

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where L: SharedDomainLift<N, Seed, H, R>,
      N: Clone + Send + Sync + 'static,
      Seed: Clone + Send + Sync + 'static,
      H: Clone + Send + Sync + 'static,
      R: Clone + Send + Sync + 'static,
      L::N2<N>: Clone + Send + Sync + 'static,
      L::Seed2<Seed>: Clone + Send + Sync + 'static,
      L::MapH<N, H, R>: Clone + Send + Sync + 'static,
      L::MapR<N, H, R>: Clone + Send + Sync + 'static,
{
    pub fn drive<T>(
        &self,
        entry_seeds: Edgy<(), L::Seed2<Seed>>,
        entry_heap_fn: impl Fn() -> L::MapH<N, H, R> + Send + Sync + 'static,
        cont: impl FnOnce(
            &Fold<
                LiftedNode<L::Seed2<Seed>, L::N2<N>>,
                LiftedHeap<L::MapH<N, H, R>, L::MapR<N, H, R>>,
                L::MapR<N, H, R>,
            >,
            &Treeish<LiftedNode<L::Seed2<Seed>, L::N2<N>>>,
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
