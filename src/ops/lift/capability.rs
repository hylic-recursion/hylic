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
///
/// Declares:
/// - `GrowXform<N2>`: stored `&N → N2` closure (for grow composition).
/// - `TreeishXform<N2>`: stored full `&Graph<N,N> → Graph<N2,N2>`
///   closure (full builder, supporting context-dependent lifts).
/// - `FoldXform<H, R, N2, H2, R2>`: stored
///   `Fold<N,H,R> → Fold<N2,H2,R2>` closure crossing `Domain<N>`
///   and `Domain<N2>` projections.
///
/// Three applicator methods evaluate the stored closures inside
/// `Lift::apply`. Three identity constructors let shape-lift
/// constructor functions fill unused axes without per-shape
/// hand-rolling identity closures.
pub trait ShapeCapable<N: 'static>: Domain<N> {
    type GrowXform<N2: 'static>: Clone + 'static;
    type TreeishXform<N2: 'static>: Clone + 'static;
    type FoldXform<H, R, N2, H2, R2>: Clone + 'static
    where H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static;

    fn apply_grow_xform<Seed: 'static, N2: 'static>(
        t: &Self::GrowXform<N2>,
        g: Self::Grow<Seed, N>,
    ) -> <Self as Domain<N2>>::Grow<Seed, N2>
    where Self: Domain<N2>;

    fn apply_treeish_xform<N2: 'static>(
        t: &Self::TreeishXform<N2>,
        g: Self::Graph<N, N>,
    ) -> <Self as Domain<N2>>::Graph<N2, N2>
    where Self: Domain<N2>;

    fn apply_fold_xform<H, R, N2, H2, R2>(
        t: &Self::FoldXform<H, R, N2, H2, R2>,
        f: Self::Fold<H, R>,
    ) -> <Self as Domain<N2>>::Fold<H2, R2>
    where Self: Domain<N2>,
          H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static;

    fn identity_grow_xform() -> Self::GrowXform<N>
    where N: Clone;

    fn identity_treeish_xform() -> Self::TreeishXform<N>
    where N: Clone;

    fn identity_fold_xform<H: 'static, R: 'static>() -> Self::FoldXform<H, R, N, H, R>;
}

// ── PureLift — sequential executor capability ────────────

/// Blanket marker: any Lift carrying `Clone + 'static` on the
/// struct and its three associated types, given the same on the
/// three input types. Sufficient for running under a sequential
/// executor (Fused) in any `ShapeCapable` domain.
pub trait PureLift<D, N, H, R>:
    Lift<D, N, H, R> + Clone + 'static
where
    D: Domain<N>,
    N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    Self::N2:   Clone + 'static,
    Self::MapH: Clone + 'static,
    Self::MapR: Clone + 'static,
{}

impl<L, D, N, H, R> PureLift<D, N, H, R> for L
where
    L: Lift<D, N, H, R> + Clone + 'static,
    D: Domain<N>,
    N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    L::N2:   Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{}

// ── ShareableLift — parallel executor capability ─────────

/// Blanket marker: adds Send+Sync on the struct and all six types.
/// Required by parallel executors (Funnel). Practically satisfied
/// only when `D = Shared` (Shared's closure storage carries Send+Sync).
pub trait ShareableLift<D, N, H, R>:
    PureLift<D, N, H, R> + Send + Sync
where
    D: Domain<N>,
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
    D: Domain<N>,
    N: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    L::N2:   Clone + Send + Sync + 'static,
    L::MapH: Clone + Send + Sync + 'static,
    L::MapR: Clone + Send + Sync + 'static,
{}
