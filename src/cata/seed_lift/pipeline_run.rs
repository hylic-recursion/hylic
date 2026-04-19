//! Pipeline execution: with_lifted CPS and run methods.
//!
//! The execution chain:
//!   constituents → compose_treeish → pre-lift L → SeedLift<Nt, Seed> → executor
//!
//! Pre-lift L: Lift<N, Nt>. Transforms (H, R) → (MapH<H,R>, MapR<H,R>).
//! SeedLift wraps into LiftedNode<Seed, Nt> / LiftedHeap<MapH, MapR>.
//! The executor's result type is MapR<H, R> (the pre-lift's output R).

use std::sync::Arc;
use crate::domain::{self, shared};
use crate::graph::{self, Edgy, Treeish};
use crate::cata::exec::Executor;
use crate::ops::Lift;
use super::types::{LiftedNode, LiftedHeap};
use super::lift::SeedLift;
use super::pipeline::SeedPipeline;

// ── Internal: late fusion ───────────────────────────

impl<N, Seed, H, R, Nt, L> SeedPipeline<N, Seed, H, R, Nt, L>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    Nt: Clone + 'static,
    H: Clone + 'static,
    R: Clone + 'static,
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

// ── CPS: the single fusion point ────────────────────

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
    pub fn with_lifted<T>(
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

// ── Run methods ─────────────────────────────────────

impl<N, Seed, H, R, Nt, L> SeedPipeline<N, Seed, H, R, Nt, L>
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + 'static,
    Nt: Clone + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + 'static,
    L: Lift<N, Nt> + Clone + Send + Sync + 'static,
    L::MapH<H, R>: Clone + Send + Sync,
    L::MapR<H, R>: Clone + Send,
{
    pub fn run(
        &self,
        exec: &impl Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
        entry_seeds: Edgy<(), Seed>,
        entry_heap: L::MapH<H, R>,
    ) -> L::MapR<H, R> {
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Entry))
    }

    pub fn run_from_slice(
        &self,
        exec: &impl Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
        seeds: &[Seed],
        entry_heap: L::MapH<H, R>,
    ) -> L::MapR<H, R>
    where Seed: Send + Sync,
    {
        let owned = seeds.to_vec();
        let entry_seeds = graph::edgy_visit(move |_: &(), cb: &mut dyn FnMut(&Seed)| {
            for s in &owned { cb(s); }
        });
        self.run(exec, entry_seeds, entry_heap)
    }

    pub fn run_seed(
        &self,
        exec: &impl Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
        seed: &Seed,
        entry_heap: L::MapH<H, R>,
    ) -> L::MapR<H, R>
    where Seed: Send + Sync,
    {
        self.run_from_slice(exec, &[seed.clone()], entry_heap)
    }

    pub fn run_node(
        &self,
        exec: &impl Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
        node: &Nt,
        entry_heap: L::MapH<H, R>,
    ) -> L::MapR<H, R> {
        let entry_seeds = graph::edgy_visit(|_: &(), _: &mut dyn FnMut(&Seed)| {});
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Node(node.clone())))
    }
}
