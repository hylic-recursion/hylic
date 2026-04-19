//! SeedPipelineExec — execution conveniences over `drive`.
//!
//! Uses `SharedDomainLift<N, Seed, H, R>` as the bound bundle. Rust
//! doesn't elaborate supertrait GAT-projection bounds to the body
//! context, so we re-state the four projections explicitly here —
//! the cost is one location, not every sugar call site.

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
      L::N2<N>: Clone + Send + Sync + 'static,
      L::Seed2<Seed>: Clone + Send + Sync + 'static,
      L::MapH<N, H, R>: Clone + Send + Sync + 'static,
      L::MapR<N, H, R>: Clone + Send + Sync + 'static,
{
    fn run<E>(
        &self,
        exec: &E,
        entry_seeds: Edgy<(), L::Seed2<Seed>>,
        entry_heap: L::MapH<N, H, R>,
    ) -> L::MapR<N, H, R>
    where E: Executor<
        LiftedNode<L::Seed2<Seed>, L::N2<N>>,
        L::MapR<N, H, R>,
        domain::Shared,
        Treeish<LiftedNode<L::Seed2<Seed>, L::N2<N>>>,
    >;

    fn run_from_slice<E>(
        &self,
        exec: &E,
        seeds: &[L::Seed2<Seed>],
        entry_heap: L::MapH<N, H, R>,
    ) -> L::MapR<N, H, R>
    where E: Executor<
        LiftedNode<L::Seed2<Seed>, L::N2<N>>,
        L::MapR<N, H, R>,
        domain::Shared,
        Treeish<LiftedNode<L::Seed2<Seed>, L::N2<N>>>,
    >;
}

impl<N, Seed, H, R, L> SeedPipelineExec<N, Seed, H, R, L> for SeedPipeline<N, Seed, H, R, L>
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
    fn run<E>(
        &self,
        exec: &E,
        entry_seeds: Edgy<(), L::Seed2<Seed>>,
        entry_heap: L::MapH<N, H, R>,
    ) -> L::MapR<N, H, R>
    where E: Executor<
        LiftedNode<L::Seed2<Seed>, L::N2<N>>,
        L::MapR<N, H, R>,
        domain::Shared,
        Treeish<LiftedNode<L::Seed2<Seed>, L::N2<N>>>,
    >,
    {
        self.drive(
            entry_seeds,
            move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Entry),
        )
    }

    fn run_from_slice<E>(
        &self,
        exec: &E,
        seeds: &[L::Seed2<Seed>],
        entry_heap: L::MapH<N, H, R>,
    ) -> L::MapR<N, H, R>
    where E: Executor<
        LiftedNode<L::Seed2<Seed>, L::N2<N>>,
        L::MapR<N, H, R>,
        domain::Shared,
        Treeish<LiftedNode<L::Seed2<Seed>, L::N2<N>>>,
    >,
    {
        let owned: Vec<L::Seed2<Seed>> = seeds.to_vec();
        let entry_seeds = graph::edgy_visit(move |_: &(), cb: &mut dyn FnMut(&L::Seed2<Seed>)| {
            for s in &owned { cb(s); }
        });
        self.run(exec, entry_seeds, entry_heap)
    }
}
