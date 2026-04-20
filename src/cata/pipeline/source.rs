//! PipelineSource and PipelineExec — the shared CPS layer.
//!
//! `PipelineSource` — sole primitive `with_constructed(cont)`
//! yielding `(grow, treeish, fold)` via CPS. Implemented by every
//! pipeline typestate (`SeedPipeline`, `TreeishPipeline`,
//! `LiftedPipeline`).
//!
//! `PipelineExec` — blanket extension with three run methods:
//!   * `run_from_node`:     sequential, minimal bounds (no Send+Sync
//!                          required on pipeline types). Skips
//!                          SeedLift — caller supplies a root in
//!                          the pipeline's effective N.
//!   * `run`:               Entry dispatch via SeedLift. Requires
//!                          the entry_heap closure to be Send+Sync,
//!                          so Self::H: Send+Sync.
//!   * `run_from_slice`:    Sugar over run with a &[Seed] source.
//!
//! Additional Send+Sync constraints on `Self::N`, `Self::R` are
//! propagated from the specific `Executor<…>` impl at the call
//! site — they live on the executor's own where-clauses, not here.

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

pub trait PipelineExec: PipelineSource {
    /// Run from a supplied root node, skipping Entry/Seed/Node
    /// dispatch. Minimal bounds — no Send+Sync on pipeline types;
    /// any Send requirements come from the specific executor.
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

    /// Run from entry seeds via SeedLift Entry dispatch. Requires
    /// pipeline types to satisfy the Shared-domain closure bounds;
    /// the entry_heap moves into a `move ||` closure that must be
    /// `Send + Sync` to satisfy `lift_fold`.
    fn run<E>(
        &self,
        exec:        &E,
        entry_seeds: Edgy<(), Self::Seed>,
        entry_heap:  Self::H,
    ) -> Self::R
    where E: Executor<
            LiftedNode<Self::Seed, Self::N>, Self::R,
            Shared, Treeish<LiftedNode<Self::Seed, Self::N>>>,
          Self::Seed: Send + Sync,
          Self::N:    Send + Sync,
          Self::H:    Send + Sync,
    {
        self.with_constructed(|grow, treeish, fold| {
            let sl = SeedLift { grow };
            let lifted_treeish = sl.lift_treeish(treeish, entry_seeds);
            let heap = entry_heap;
            let lifted_fold = sl.lift_fold(fold, move || heap.clone());
            exec.run(&lifted_fold, &lifted_treeish, &LiftedNode::Entry)
        })
    }

    /// Sugar over `run` with a `&[Seed]` source.
    fn run_from_slice<E>(
        &self,
        exec:       &E,
        seeds:      &[Self::Seed],
        entry_heap: Self::H,
    ) -> Self::R
    where E: Executor<
            LiftedNode<Self::Seed, Self::N>, Self::R,
            Shared, Treeish<LiftedNode<Self::Seed, Self::N>>>,
          Self::Seed: Send + Sync,
          Self::N:    Send + Sync,
          Self::H:    Send + Sync,
    {
        let owned: Vec<Self::Seed> = seeds.to_vec();
        let entry_seeds: Edgy<(), Self::Seed> = graph::edgy_visit(
            move |_: &(), cb: &mut dyn FnMut(&Self::Seed)| {
                for s in &owned { cb(s); }
            }
        );
        self.run(exec, entry_seeds, entry_heap)
    }
}

impl<P: PipelineSource> PipelineExec for P {}
