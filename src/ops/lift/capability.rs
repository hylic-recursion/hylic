// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Capability traits for lifts.
//!
//! `ShapeCapable<N>: Domain<N>` extends a domain with xform storage
//! types + applicators + identity constructors, enabling
//! `ShapeLift<D, N, H, R, N2, H2, R2>` composition. Implemented by
//! `Shared` and `Local`; NOT by `Owned` (see
//! `technical-insights/09-unified-shape-lift.md`).
//!
//! `PureLift<D, N, H, R>` — blanket marker: any Lift that is
//! Clone + 'static with Clone + 'static outputs. Sufficient for
//! sequential executors on any capable domain.
//!
//! `ShareableLift<D, N, H, R>` — blanket marker: adds Send + Sync
//! bounds on everything. Required for parallel executors (Funnel);
//! practically only `D = Shared` satisfies this.

use crate::domain::Domain;
use super::core::Lift;

// ── ShapeCapable ─────────────────────────────────────────

/// A domain that supports `ShapeLift` composition.
pub trait ShapeCapable<N: 'static>: Domain<N> {
    type GrowXform<N2: 'static>: Clone + 'static;
    type TreeishXform<N2: 'static>: Clone + 'static;
    type FoldXform<H, R, N2, H2, R2>: Clone + 'static
    where H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static;

    fn apply_grow_xform<Seed: 'static, N2: 'static>(
        t: &Self::GrowXform<N2>,
        g: <Self as Domain<N>>::Grow<Seed, N>,
    ) -> <Self as Domain<N2>>::Grow<Seed, N2>
    where Self: Domain<N2>;

    fn apply_treeish_xform<N2: 'static>(
        t: &Self::TreeishXform<N2>,
        g: <Self as Domain<N>>::Graph<N>,
    ) -> <Self as Domain<N2>>::Graph<N2>
    where Self: Domain<N2>;

    fn apply_fold_xform<H, R, N2, H2, R2>(
        t: &Self::FoldXform<H, R, N2, H2, R2>,
        f: <Self as Domain<N>>::Fold<H, R>,
    ) -> <Self as Domain<N2>>::Fold<H2, R2>
    where Self: Domain<N2>,
          H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static;

    fn identity_grow_xform() -> Self::GrowXform<N>
    where N: Clone;

    fn identity_treeish_xform() -> Self::TreeishXform<N>
    where N: Clone;

    fn identity_fold_xform<H: 'static, R: 'static>() -> Self::FoldXform<H, R, N, H, R>;

    /// Compose a `grow: Seed → N` with a `seeds: Graph<Seed>` to
    /// produce the fused `Graph<N>` (treeish). Needed by
    /// `SeedPipeline::with_constructed` which yields a treeish over
    /// N to the executor.
    fn fuse_grow_with_seeds<Seed: 'static>(
        grow:  <Self as Domain<N>>::Grow<Seed, N>,
        seeds: <Self as Domain<N>>::Graph<Seed>,
    ) -> <Self as Domain<N>>::Graph<N>
    where Seed: Clone;
}

// ── PureLift — sequential executor capability ────────────

pub trait PureLift<D, N, H, R>:
    Lift<D, N, H, R> + Clone + 'static
where
    D: Domain<N> + Domain<Self::N2>,
    N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    Self::N2:   Clone + 'static,
    Self::MapH: Clone + 'static,
    Self::MapR: Clone + 'static,
{}

impl<L, D, N, H, R> PureLift<D, N, H, R> for L
where
    L: Lift<D, N, H, R> + Clone + 'static,
    D: Domain<N> + Domain<L::N2>,
    N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    L::N2:   Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{}

// ── ShareableLift — parallel executor capability ─────────

pub trait ShareableLift<D, N, H, R>:
    PureLift<D, N, H, R> + Send + Sync
where
    D: Domain<N> + Domain<Self::N2>,
    N: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    Self::N2:   Clone + Send + Sync + 'static,
    Self::MapH: Clone + Send + Sync + 'static,
    Self::MapR: Clone + Send + Sync + 'static,
{}

impl<L, D, N, H, R> ShareableLift<D, N, H, R> for L
where
    L: PureLift<D, N, H, R> + Send + Sync,
    D: Domain<N> + Domain<L::N2>,
    N: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    L::N2:   Clone + Send + Sync + 'static,
    L::MapH: Clone + Send + Sync + 'static,
    L::MapR: Clone + Send + Sync + 'static,
{}
