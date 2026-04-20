//! PipelineSource and PipelineExec — the shared CPS layer.
//!
//! `PipelineSource` — sole primitive `with_constructed(cont)`
//! yielding `(grow, treeish, fold)` via CPS, all in the domain's
//! native storage. Implemented by every by-reference pipeline
//! typestate (`SeedPipeline`, `TreeishPipeline`, `LiftedPipeline`).
//!
//! `PipelineSourceOnce` — sibling trait for pipelines that must
//! be consumed to yield their triple (used by `OwnedPipeline`).
//! Mutually exclusive in practice: a pipeline implements one or
//! the other, not both.
//!
//! `PipelineExec` — blanket extension with three run methods:
//!   * `run_from_node`: sequential, any capable domain. No
//!     Send+Sync requirements on pipeline types.
//!   * `run`: Entry dispatch via SeedLift — Shared-only pending
//!     Phase 5/6 generalisation.
//!   * `run_from_slice`: Sugar over `run` with a &[Seed] source.
//!
//! `PipelineExecOnce` — the by-value analogue for OwnedPipeline:
//!   * `run_from_node_once`: consumes self; no SeedLift.

use std::sync::Arc;
use crate::cata::exec::Executor;
use crate::domain::{Domain, Shared};
use crate::domain::shared::fold::Fold;
use crate::graph::{self, Edgy, Treeish};
use crate::ops::TreeOps;
use super::internal::{LiftedNode, SeedLift};

// ── PipelineSource ─────────────────────────────────────

pub trait PipelineSource {
    type Domain: Domain<Self::N>;
    type Seed: Clone + 'static;
    type N:    Clone + 'static;
    type H:    Clone + 'static;
    type R:    Clone + 'static;

    fn with_constructed<T>(
        &self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Grow<Self::Seed, Self::N>,
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T;
}

// ── PipelineSourceOnce ─────────────────────────────────

pub trait PipelineSourceOnce {
    type Domain: Domain<Self::N>;
    type Seed: 'static;
    type N:    'static;
    type H:    'static;
    type R:    'static;

    fn with_constructed_once<T>(
        self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Grow<Self::Seed, Self::N>,
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T;
}

// ── PipelineExec ───────────────────────────────────────

pub trait PipelineExec: PipelineSource {
    /// Run from a supplied root node, skipping Entry/Seed/Node
    /// dispatch. Works on any capable domain.
    fn run_from_node<E>(
        &self,
        exec: &E,
        root: &Self::N,
    ) -> Self::R
    where E: Executor<
            Self::N, Self::R, Self::Domain,
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
        >,
          <Self::Domain as Domain<Self::N>>::Graph<Self::N>: TreeOps<Self::N>,
    {
        self.with_constructed(|_grow, treeish, fold| {
            exec.run(&fold, &treeish, root)
        })
    }

    /// Run from entry seeds via SeedLift Entry dispatch. Requires
    /// the pipeline's domain to be Shared (SeedLift pinned pending
    /// Phase 5/6 generalisation).
    fn run<E>(
        &self,
        exec:        &E,
        entry_seeds: Edgy<(), Self::Seed>,
        entry_heap:  Self::H,
    ) -> Self::R
    where Self: PipelineSource<Domain = Shared>,
          E: Executor<
            LiftedNode<Self::Seed, Self::N>, Self::R,
            Shared, Treeish<LiftedNode<Self::Seed, Self::N>>>,
          Self::Seed: Send + Sync,
          Self::N:    Send + Sync,
          Self::H:    Send + Sync,
    {
        self.with_constructed(|grow, treeish, fold| {
            let grow: Arc<dyn Fn(&Self::Seed) -> Self::N + Send + Sync> = grow;
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
    where Self: PipelineSource<Domain = Shared>,
          E: Executor<
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

// ── PipelineExecOnce ───────────────────────────────────

pub trait PipelineExecOnce: PipelineSourceOnce + Sized {
    fn run_from_node_once<E>(
        self,
        exec: &E,
        root: &Self::N,
    ) -> Self::R
    where E: Executor<
            Self::N, Self::R, Self::Domain,
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
        >,
          <Self::Domain as Domain<Self::N>>::Graph<Self::N>: TreeOps<Self::N>,
          Self::N: Clone,
    {
        self.with_constructed_once(|_grow, treeish, fold| {
            exec.run(&fold, &treeish, root)
        })
    }
}

impl<P: PipelineSourceOnce + Sized> PipelineExecOnce for P {}

// Allow compatibility references to old Fold type in doc-comments etc.
#[allow(dead_code)]
type _SharedFold<N, H, R> = Fold<N, H, R>;
