//! SeedPipelineExec — execution conveniences over `drive`.

use crate::domain;
use crate::graph::{self, Edgy, Treeish};
use crate::cata::exec::Executor;
use crate::ops::{Lift as _, SharedDomainLift};
use super::super::types::LiftedNode;
use super::core::SeedPipeline;

pub trait SeedPipelineExec<N, Seed, H, R, L>
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
    fn run<E>(&self, exec: &E, entry_seeds: Edgy<(), L::Seed2>, entry_heap: L::MapH) -> L::MapR
    where E: Executor<
        LiftedNode<L::Seed2, L::N2>, L::MapR,
        domain::Shared, Treeish<LiftedNode<L::Seed2, L::N2>>>;

    fn run_from_slice<E>(&self, exec: &E, seeds: &[L::Seed2], entry_heap: L::MapH) -> L::MapR
    where E: Executor<
        LiftedNode<L::Seed2, L::N2>, L::MapR,
        domain::Shared, Treeish<LiftedNode<L::Seed2, L::N2>>>;
}

impl<N, Seed, H, R, L> SeedPipelineExec<N, Seed, H, R, L> for SeedPipeline<N, Seed, H, R, L>
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
    fn run<E>(&self, exec: &E, entry_seeds: Edgy<(), L::Seed2>, entry_heap: L::MapH) -> L::MapR
    where E: Executor<
        LiftedNode<L::Seed2, L::N2>, L::MapR,
        domain::Shared, Treeish<LiftedNode<L::Seed2, L::N2>>>,
    {
        self.drive(
            entry_seeds,
            move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Entry),
        )
    }

    fn run_from_slice<E>(&self, exec: &E, seeds: &[L::Seed2], entry_heap: L::MapH) -> L::MapR
    where E: Executor<
        LiftedNode<L::Seed2, L::N2>, L::MapR,
        domain::Shared, Treeish<LiftedNode<L::Seed2, L::N2>>>,
    {
        let owned: Vec<L::Seed2> = seeds.to_vec();
        let entry_seeds = graph::edgy_visit(move |_: &(), cb: &mut dyn FnMut(&L::Seed2)| {
            for s in &owned { cb(s); }
        });
        self.run(exec, entry_seeds, entry_heap)
    }
}
