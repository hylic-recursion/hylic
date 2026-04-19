//! PipelineSource and PipelineExec — the shared CPS layer.
//!
//! PipelineSource:  sole primitive `with_constructed(cont)` yielding
//!                  (grow, treeish, fold) via CPS. Implemented by
//!                  both SeedPipeline and LiftedPipeline.
//!
//! PipelineExec:    blanket-implemented extension providing `run`,
//!                  `run_from_slice`, `run_from_node`. The SeedLift
//!                  post-composition (Entry/Seed/Node dispatch) lives
//!                  inside run / run_from_slice; run_from_node skips it.

use std::sync::Arc;
use crate::cata::exec::Executor;
use crate::domain::shared::fold::Fold;
use crate::domain::Shared;
use crate::graph::{self, Edgy, Treeish};
use super::internal::{LiftedNode, SeedLift};

// ── PipelineSource ─────────────────────────────────────

pub trait PipelineSource {
    type Seed: Clone + 'static;
    type N:    Clone + 'static;
    type H:    Clone + 'static;
    type R:    Clone + 'static;

    fn with_constructed<T>(
        &self,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed) -> Self::N + Send + Sync>,
            Treeish<Self::N>,
            Fold<Self::N, Self::H, Self::R>,
        ) -> T,
    ) -> T;
}

// ── PipelineExec ───────────────────────────────────────
//
// Extension trait; blanket impl below for any PipelineSource whose
// effective types satisfy Shared-domain bounds.

pub trait PipelineExec: PipelineSource
where Self::Seed: Clone + Send + Sync + 'static,
      Self::N:    Clone + Send + Sync + 'static,
      Self::H:    Clone + Send + Sync + 'static,
      Self::R:    Clone + Send + Sync + 'static,
{
    fn run<E>(
        &self,
        exec:        &E,
        entry_seeds: Edgy<(), Self::Seed>,
        entry_heap:  Self::H,
    ) -> Self::R
    where E: Executor<
        LiftedNode<Self::Seed, Self::N>, Self::R,
        Shared, Treeish<LiftedNode<Self::Seed, Self::N>>>,
    {
        self.with_constructed(|grow, treeish, fold| {
            let sl = SeedLift { grow };
            let lifted_treeish = sl.lift_treeish(treeish, entry_seeds);
            let heap = entry_heap;
            let lifted_fold = sl.lift_fold(fold, move || heap.clone());
            exec.run(&lifted_fold, &lifted_treeish, &LiftedNode::Entry)
        })
    }

    fn run_from_slice<E>(
        &self,
        exec:       &E,
        seeds:      &[Self::Seed],
        entry_heap: Self::H,
    ) -> Self::R
    where E: Executor<
        LiftedNode<Self::Seed, Self::N>, Self::R,
        Shared, Treeish<LiftedNode<Self::Seed, Self::N>>>,
    {
        let owned: Vec<Self::Seed> = seeds.to_vec();
        let entry_seeds: Edgy<(), Self::Seed> = graph::edgy_visit(
            move |_: &(), cb: &mut dyn FnMut(&Self::Seed)| {
                for s in &owned { cb(s); }
            }
        );
        self.run(exec, entry_seeds, entry_heap)
    }

    fn run_from_node<E>(
        &self,
        exec: &E,
        root: &Self::N,
    ) -> Self::R
    where E: Executor<Self::N, Self::R, Shared, Treeish<Self::N>>,
    {
        self.with_constructed(|_grow, treeish, fold| {
            exec.run(&fold, &treeish, root)
        })
    }
}

impl<P: PipelineSource> PipelineExec for P
where P::Seed: Clone + Send + Sync + 'static,
      P::N:    Clone + Send + Sync + 'static,
      P::H:    Clone + Send + Sync + 'static,
      P::R:    Clone + Send + Sync + 'static,
{}

