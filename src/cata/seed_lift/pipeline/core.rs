//! SeedPipeline core: struct definition and the two primitives.
//!
//! `map_constituents` is the only type-reshape primitive (consumes self,
//! zero trait bounds beyond the new-type 'static floor).
//!
//! `drive` is the only CPS execution primitive, carrying the minimum
//! bounds its body requires. Named execution conveniences
//! (`run`, `run_from_slice`, …) live in the `exec` module as a trait.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::domain::shared;
use crate::graph::{Edgy, Treeish};
use crate::ops::{IdentityLift, Lift};
use super::super::types::{LiftedHeap, LiftedNode};
use super::super::lift::SeedLift;

// ── Struct ──────────────────────────────────────────

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

// ── Constructor ─────────────────────────────────────

impl<N: 'static, Seed: 'static, H: 'static, R: 'static>
    SeedPipeline<N, Seed, H, R, N, IdentityLift>
{
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

// ── map_constituents: the only type-reshape primitive ──
//
// Consumes self. Zero trait bounds beyond `'static` on the new type
// parameters (imposed by the stored trait objects).

impl<N: 'static, Seed: 'static, H: 'static, R: 'static, Nt: 'static, L>
    SeedPipeline<N, Seed, H, R, Nt, L>
{
    pub fn map_constituents<N2: 'static, Seed2: 'static, H2: 'static, R2: 'static, Nt2: 'static, L2>(
        self,
        map_grow: impl FnOnce(Arc<dyn Fn(&Seed) -> N + Send + Sync>)
                            -> Arc<dyn Fn(&Seed2) -> N2 + Send + Sync>,
        map_seeds: impl FnOnce(Edgy<N, Seed>) -> Edgy<N2, Seed2>,
        map_fold: impl FnOnce(shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, H2, R2>,
        map_pre_lift: impl FnOnce(L) -> L2,
    ) -> SeedPipeline<N2, Seed2, H2, R2, Nt2, L2> {
        SeedPipeline {
            grow: map_grow(self.grow),
            seeds_from_node: map_seeds(self.seeds_from_node),
            fold: map_fold(self.fold),
            pre_lift: map_pre_lift(self.pre_lift),
            _nt: PhantomData,
        }
    }
}

// ── drive: the only CPS execution primitive ──────────
//
// Composes the treeish + fold through the pre-lift chain and SeedLift,
// hands the result pair to a user-provided continuation. Bounds are
// the minimum required by the body.

impl<N, Seed, H, R, Nt, L> SeedPipeline<N, Seed, H, R, Nt, L>
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + 'static,
    Nt: Clone + 'static,
    H: Clone + 'static,
    R: Clone + 'static,
    L: Lift<N, Nt> + Clone + Send + Sync + 'static,
    L::MapH<H, R>: Clone,
    L::MapR<H, R>: Clone,
{
    pub fn drive<T>(
        &self,
        entry_seeds: Edgy<(), Seed>,
        entry_heap_fn: impl Fn() -> L::MapH<H, R> + Send + Sync + 'static,
        cont: impl FnOnce(
            &shared::fold::Fold<LiftedNode<Seed, Nt>, LiftedHeap<L::MapH<H, R>, L::MapR<H, R>>, L::MapR<H, R>>,
            &Treeish<LiftedNode<Seed, Nt>>,
        ) -> T,
    ) -> T {
        let treeish = self.compose_treeish();
        let pre_treeish = self.pre_lift.lift_treeish(treeish);
        let pre_fold = self.pre_lift.lift_fold(self.fold.clone());
        let pre_grow = self.compose_pre_grow();
        let seed_lift = SeedLift { grow: pre_grow };
        let lifted_treeish = seed_lift.lift_treeish(pre_treeish, entry_seeds);
        let lifted_fold = seed_lift.lift_fold(pre_fold, entry_heap_fn);
        cont(&lifted_fold, &lifted_treeish)
    }
}

// ── drive's internal helpers ────────────────────────

impl<N, Seed, H, R, Nt, L> SeedPipeline<N, Seed, H, R, Nt, L>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    Nt: Clone + 'static,
    L: Lift<N, Nt> + Clone + Send + Sync + 'static,
{
    // ANCHOR: treeish_from_seeds
    fn compose_treeish(&self) -> Treeish<N> {
        self.seeds_from_node.map({
            let g = self.grow.clone();
            move |seed: &Seed| g(seed)
        })
    }
    // ANCHOR_END: treeish_from_seeds

    fn compose_pre_grow(&self) -> Arc<dyn Fn(&Seed) -> Nt + Send + Sync> {
        let grow = self.grow.clone();
        let lift = self.pre_lift.clone();
        Arc::new(move |s: &Seed| lift.lift_root(&grow(s)))
    }
}
