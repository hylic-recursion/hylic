//! Pipeline execution: with_lifted CPS and run methods.
//!
//! The execution chain:
//!   constituents → compose_treeish → pre-lift L → SeedLift → executor

use std::sync::Arc;
use crate::domain::{self, shared};
use crate::graph::{self, Edgy, Treeish};
use crate::cata::exec::Executor;
use crate::ops::LiftOps;
use super::types::{LiftedNode, LiftedHeap};
use super::lift::SeedLift;
use super::pipeline::SeedPipeline;

// ── Internal: late fusion ───────────────────────────

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    H: 'static,
    R: Clone + 'static,
    L: LiftOps<N, R, N>,  // pre-lift (IdentityLift satisfies this)
{
    // ANCHOR: treeish_from_seeds
    fn compose_treeish(&self) -> Treeish<N> {
        self.seeds_from_node.map({
            let g = self.grow.clone();
            move |seed: &Seed| g(seed)
        })
    }
    // ANCHOR_END: treeish_from_seeds
}

// ── CPS: the single fusion point ────────────────────
//
// The pre-lift L transforms (Treeish<N>, Fold<N,H,R>) into
// (Treeish<Nt>, Fold<Nt, Ht, Rt>). Then SeedLift wraps into
// LiftedNode<Seed, Nt>.
//
// For IdentityLift: Nt=N, Ht=H, Rt=R (pass-through).

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + 'static,
    H: Clone + 'static,
    R: Clone + 'static,
    L: LiftOps<N, R, N>,
    // The pre-lift's output node type. For IdentityLift, this is N.
    // For a type-changing pre-lift, this would be Nt ≠ N.
    // Currently constrained to N (same type) for simplicity.
    // TODO: generalize to L: LiftOps<N, R, Nt> with separate Nt
{
    pub fn with_lifted<T>(
        &self,
        entry_seeds: Edgy<(), Seed>,
        entry_heap_fn: impl Fn() -> L::LiftedH<H> + Send + Sync + 'static,
        cont: impl FnOnce(
            &shared::fold::Fold<LiftedNode<Seed, N>, LiftedHeap<L::LiftedH<H>, L::LiftedR<H>>, L::LiftedR<H>>,
            &Treeish<LiftedNode<Seed, N>>,
        ) -> T,
    ) -> T
    where
        L::LiftedH<H>: Clone,
        L::LiftedR<H>: Clone,
    {
        let treeish = self.compose_treeish();
        // Apply pre-lift
        let pre_treeish = self.pre_lift.lift_treeish(treeish);
        let pre_fold = self.pre_lift.lift_fold(self.fold.clone());
        // Grow must also go through pre-lift
        let pre_grow: Arc<dyn Fn(&Seed) -> N + Send + Sync> = {
            let grow = self.grow.clone();
            // For IdentityLift, lift_root is identity. For type-changing
            // pre-lifts, this composes grow with the node transform.
            // TODO: when Nt ≠ N, this needs Arc<dyn Fn(&Seed) -> Nt>
            grow
        };
        let seed_lift = SeedLift { grow: pre_grow };
        let lifted_treeish = seed_lift.lift_treeish(pre_treeish, entry_seeds);
        let lifted_fold = seed_lift.lift_fold(pre_fold, entry_heap_fn);
        cont(&lifted_fold, &lifted_treeish)
    }
}

// ── Run methods ─────────────────────────────────────

impl<N, Seed, H, R, L> SeedPipeline<N, Seed, H, R, L>
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + 'static,
    L: LiftOps<N, R, N>,
    L::LiftedH<H>: Clone + Send + Sync,
    L::LiftedR<H>: Clone + Send,
{
    pub fn run(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, L::LiftedR<H>, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        entry_seeds: Edgy<(), Seed>,
        entry_heap: L::LiftedH<H>,
    ) -> L::LiftedR<H> {
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Entry))
    }

    pub fn run_from_slice(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, L::LiftedR<H>, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        seeds: &[Seed],
        entry_heap: L::LiftedH<H>,
    ) -> L::LiftedR<H>
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
        exec: &impl Executor<LiftedNode<Seed, N>, L::LiftedR<H>, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        seed: &Seed,
        entry_heap: L::LiftedH<H>,
    ) -> L::LiftedR<H>
    where Seed: Send + Sync,
    {
        self.run_from_slice(exec, &[seed.clone()], entry_heap)
    }

    pub fn run_node(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, L::LiftedR<H>, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        node: &N,
        entry_heap: L::LiftedH<H>,
    ) -> L::LiftedR<H> {
        let entry_seeds = graph::edgy_visit(|_: &(), _: &mut dyn FnMut(&Seed)| {});
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Node(node.clone())))
    }
}
