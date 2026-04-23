// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Pipeline source traits and execution extensions.
//!
//! The source hierarchy has two axes: **by-reference vs by-value**
//! (consume semantics) and **seedless vs seeded** (Entry-dispatch
//! capability).
//!
//!                  seedless              seeded
//!                  ─────────             ─────────────────
//!   by-reference   TreeishSource   ◀─── SeedSource
//!                                       (extends TreeishSource)
//!   by-value       PipelineSourceOnce
//!
//! `TreeishSource` is the common supertrait for by-reference
//! pipelines: yields `(treeish, fold)` in the domain's native
//! storage. No Seed semantics.
//!
//! `SeedSource: TreeishSource` extends with a `Seed` type and a
//! `with_seeded` yield that provides `(grow, treeish, fold)` — the
//! triple SeedLift needs to build Entry/Seed/Node dispatch.
//!
//! Execution:
//!   - `PipelineExec: TreeishSource`    — `run_from_node` (any pipeline)
//!   - `PipelineExecSeed: SeedSource`   — `run` + `run_from_slice` (SeedLift-composing)
//!   - `PipelineExecOnce: PipelineSourceOnce` — by-value analogue
//!
//! Pipelines that don't carry Seed semantics (TreeishPipeline) do
//! NOT inherit `.run(...)` — that's a compile-time guarantee.

use std::sync::Arc;
use crate::cata::exec::Executor;
use crate::domain::{Domain, Shared};
use crate::graph::{self, Edgy, Treeish};
use crate::ops::{Lift, LiftedNode, SeedLift, TreeOps};

// ── TreeishSource ─────────────────────────────────────

/// A by-reference pipeline that yields `(treeish, fold)` for
/// execution. Seed-agnostic.
pub trait TreeishSource {
    type Domain: Domain<Self::N>;
    type N: Clone + 'static;
    type H: Clone + 'static;
    type R: Clone + 'static;

    fn with_treeish<T>(
        &self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T;
}

// ── SeedSource ────────────────────────────────────────

/// Extends `TreeishSource` with a `Seed` type and a 3-slot yield
/// `(grow, treeish, fold)`. Implemented by pipelines that can
/// compose SeedLift for Entry dispatch.
pub trait SeedSource: TreeishSource {
    type Seed: Clone + 'static;

    fn with_seeded<T>(
        &self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Grow<Self::Seed, Self::N>,
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T;
}

// ── PipelineSourceOnce ────────────────────────────────

/// By-value analogue (OwnedPipeline). Seedless.
pub trait PipelineSourceOnce {
    type Domain: Domain<Self::N>;
    type N:    'static;
    type H:    'static;
    type R:    'static;

    fn with_constructed_once<T>(
        self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T;
}

// ── PipelineExec ──────────────────────────────────────

/// Run-from-root execution on any `TreeishSource`.
pub trait PipelineExec: TreeishSource {
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
        self.with_treeish(|treeish, fold| {
            exec.run(&fold, &treeish, root)
        })
    }
}

impl<P: TreeishSource> PipelineExec for P {}

// ── PipelineExecSeed ──────────────────────────────────

/// Entry-dispatch execution. Only available on `SeedSource` pipelines.
pub trait PipelineExecSeed: SeedSource {
    /// Run from entry seeds via a finishing `SeedLift`. Shared-pinned.
    fn run<E>(
        &self,
        exec:        &E,
        entry_seeds: Edgy<(), Self::Seed>,
        entry_heap:  Self::H,
    ) -> Self::R
    where Self: SeedSource<Domain = Shared>,
          E: Executor<
            LiftedNode<Self::N>, Self::R,
            Shared, Treeish<LiftedNode<Self::N>>>,
          Self::Seed: Send + Sync,
          Self::N:    Send + Sync,
          Self::H:    Send + Sync,
    {
        self.with_seeded(|grow, treeish, fold| {
            let grow: Arc<dyn Fn(&Self::Seed) -> Self::N + Send + Sync> = grow;
            let sl: SeedLift<Self::N, Self::Seed, Self::H> =
                SeedLift::from_arc_grow(grow.clone(), entry_seeds, move || entry_heap.clone());
            sl.apply::<Self::Seed, _>(
                grow, treeish, fold,
                |_unreachable_grow, lifted_treeish, lifted_fold| {
                    exec.run(&lifted_fold, &lifted_treeish, &LiftedNode::Entry)
                },
            )
        })
    }

    /// Sugar: wraps a `&[Seed]` slice into the canonical
    /// `Edgy<(), Seed>` callback-iterator form.
    fn run_from_slice<E>(
        &self,
        exec:       &E,
        seeds:      &[Self::Seed],
        entry_heap: Self::H,
    ) -> Self::R
    where Self: SeedSource<Domain = Shared>,
          E: Executor<
            LiftedNode<Self::N>, Self::R,
            Shared, Treeish<LiftedNode<Self::N>>>,
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

impl<P: SeedSource> PipelineExecSeed for P {}

// ── PipelineExecOnce ──────────────────────────────────

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
        self.with_constructed_once(|treeish, fold| {
            exec.run(&fold, &treeish, root)
        })
    }
}

impl<P: PipelineSourceOnce + Sized> PipelineExecOnce for P {}
