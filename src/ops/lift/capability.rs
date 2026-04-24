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
// ANCHOR: shape_capable
#[allow(missing_docs)] // associated types/methods are implementation plumbing for ShapeLift
pub trait ShapeCapable<N: 'static>: Domain<N> {
    type GrowXform<N2: 'static>: Clone + 'static;
    type TreeishXform<N2: 'static>: Clone + 'static;
    type FoldXform<H, R, N2, H2, R2>: Clone + 'static
    where H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static;

    /// Forward entry-node map: `N → N2`. Stored as a domain-native
    /// shared handle so `ShapeLift::project_entry_node` can call it.
    type EntryNodeXform<N2: 'static>: Clone + 'static;

    /// Forward entry-heap map: `H → H2`. Stored similarly.
    type EntryHeapXform<H: 'static, H2: 'static>: Clone + 'static;

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

    /// Apply the entry-node xform to lift an `N` value into `N2`.
    fn apply_entry_node_xform<N2: 'static>(
        t: &Self::EntryNodeXform<N2>,
        n: N,
    ) -> N2;

    /// Apply the entry-heap xform to lift an `H` value into `H2`.
    fn apply_entry_heap_xform<H: 'static, H2: 'static>(
        t: &Self::EntryHeapXform<H, H2>,
        h: H,
    ) -> H2;

    fn identity_grow_xform() -> Self::GrowXform<N>
    where N: Clone;

    fn identity_treeish_xform() -> Self::TreeishXform<N>
    where N: Clone;

    fn identity_fold_xform<H: 'static, R: 'static>() -> Self::FoldXform<H, R, N, H, R>;

    /// Identity entry-node xform: returns `N` unchanged. Used by
    /// N-preserving shape lifts.
    fn identity_entry_node_xform() -> Self::EntryNodeXform<N>
    where N: Clone + 'static;

    /// Identity entry-heap xform: returns `H` unchanged. Used by
    /// H-preserving shape lifts.
    fn identity_entry_heap_xform<H: Clone + 'static>()
        -> Self::EntryHeapXform<H, H>;

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

// ANCHOR_END: shape_capable

// ── PureLift — sequential executor capability ────────────

/// Blanket marker for lifts that satisfy the bounds needed by the
/// sequential executor (`Fused`): `Lift + Clone + 'static` with
/// `Clone + 'static` on every output type.
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

/// Blanket marker for lifts that additionally satisfy the
/// `Send + Sync` bounds required by parallel executors (`Funnel`,
/// `ParLazy`, `ParEager`).
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
