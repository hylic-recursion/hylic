//! Pipeline execution: with_lifted CPS and run methods.

use crate::domain::{self, shared};
use crate::graph::{self, Edgy, Treeish};
use crate::cata::exec::Executor;
use super::types::{LiftedNode, LiftedHeap};
use super::lift::SeedLift;
use super::pipeline::SeedPipeline;

// ── Internal: late fusion ───────────────────────────

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    H: 'static,
    R: Clone + 'static,
{
    // ANCHOR: treeish_from_seeds
    fn compose_treeish(&self) -> Treeish<N> {
        self.seeds_from_node.map({
            let g = self.grow.clone();
            move |seed: &Seed| g(seed)
        })
    }
    // ANCHOR_END: treeish_from_seeds

    fn make_seed_lift(&self) -> SeedLift<N, Seed> {
        SeedLift { grow: self.grow.clone() }
    }
}

// ── CPS: the single fusion point ────────────────────

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    H: 'static,
    R: Clone + 'static,
{
    /// Compose all parts, lift into the LiftedNode graph, pass the
    /// artifacts to a continuation. entry_heap_fn produces H on
    /// demand — H needs no bounds beyond 'static.
    pub fn with_lifted<T>(
        &self,
        entry_seeds: Edgy<(), Seed>,
        entry_heap_fn: impl Fn() -> H + Send + Sync + 'static,
        cont: impl FnOnce(
            &shared::fold::Fold<LiftedNode<Seed, N>, LiftedHeap<H, R>, R>,
            &Treeish<LiftedNode<Seed, N>>,
        ) -> T,
    ) -> T {
        let treeish = self.compose_treeish();
        let seed_lift = self.make_seed_lift();
        let lifted_fold = seed_lift.lift_fold(self.fold.clone(), entry_heap_fn);
        let lifted_treeish = seed_lift.lift_treeish(treeish, entry_seeds);
        cont(&lifted_fold, &lifted_treeish)
    }
}

// ── Run methods ─────────────────────────────────────

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + 'static,
{
    pub fn run(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        entry_seeds: Edgy<(), Seed>,
        entry_heap: H,
    ) -> R {
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Entry))
    }

    pub fn run_from_slice(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        seeds: &[Seed],
        entry_heap: H,
    ) -> R
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
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        seed: &Seed,
        entry_heap: H,
    ) -> R
    where Seed: Send + Sync,
    {
        self.run_from_slice(exec, &[seed.clone()], entry_heap)
    }

    pub fn run_node(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        node: &N,
        entry_heap: H,
    ) -> R {
        let entry_seeds = graph::edgy_visit(|_: &(), _: &mut dyn FnMut(&Seed)| {});
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Node(node.clone())))
    }
}
