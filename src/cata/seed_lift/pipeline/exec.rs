//! SeedPipelineExec: execution conveniences as an extension trait.
//!
//! All executor-imposed bounds (`Send`/`Sync` for parallel fan-out,
//! factory `Sync` for value-taking entry-heap sugar) live only here.
//! The base `SeedPipeline` impl stays minimal.
//!
//! Users get method syntax via `use hylic::prelude::SeedPipelineExec`
//! (or the seed_lift module's re-export).

use crate::domain;
use crate::graph::{self, Edgy, Treeish};
use crate::cata::exec::Executor;
use crate::ops::Lift;
use super::super::types::LiftedNode;
use super::core::SeedPipeline;

// ANCHOR: exec_trait
pub trait SeedPipelineExec<N, Seed, H, R, Nt, L>
where
    L: Lift<N, Nt>,
    H: Clone + 'static,
    R: Clone + 'static,
    N: 'static,
    Seed: 'static,
    Nt: 'static,
{
    fn run<E>(&self, exec: &E, entry_seeds: Edgy<(), Seed>, entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>;

    fn run_from_slice<E>(&self, exec: &E, seeds: &[Seed], entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
          Seed: Send + Sync;

    fn run_seed<E>(&self, exec: &E, seed: &Seed, entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
          Seed: Send + Sync;

    fn run_node<E>(&self, exec: &E, node: &Nt, entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>;
}
// ANCHOR_END: exec_trait

impl<N, Seed, H, R, Nt, L> SeedPipelineExec<N, Seed, H, R, Nt, L>
    for SeedPipeline<N, Seed, H, R, Nt, L>
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
    fn run<E>(&self, exec: &E, entry_seeds: Edgy<(), Seed>, entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
    {
        self.drive(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Entry))
    }

    fn run_from_slice<E>(&self, exec: &E, seeds: &[Seed], entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
          Seed: Send + Sync,
    {
        let owned = seeds.to_vec();
        let entry_seeds = graph::edgy_visit(move |_: &(), cb: &mut dyn FnMut(&Seed)| {
            for s in &owned { cb(s); }
        });
        self.run(exec, entry_seeds, entry_heap)
    }

    fn run_seed<E>(&self, exec: &E, seed: &Seed, entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
          Seed: Send + Sync,
    {
        self.run_from_slice(exec, &[seed.clone()], entry_heap)
    }

    fn run_node<E>(&self, exec: &E, node: &Nt, entry_heap: L::MapH<H, R>) -> L::MapR<H, R>
    where E: Executor<LiftedNode<Seed, Nt>, L::MapR<H, R>, domain::Shared, Treeish<LiftedNode<Seed, Nt>>>,
    {
        let entry_seeds = graph::edgy_visit(|_: &(), _: &mut dyn FnMut(&Seed)| {});
        self.drive(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Node(node.clone())))
    }
}
