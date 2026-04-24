//! Lift<D, N, H, R> — the domain-generic algebra-transform type class.
//!
//! Four trait parameters: D (domain), N (node type), H (heap), R
//! (result). Three associated types (N2, MapH, MapR). One CPS method
//! `apply`. `Seed` is method-level.
//!
//! The trait bounds declare `D: Domain<Self::N2>` at trait level —
//! via a `where` clause on the trait head — so every Lift impl can
//! assume it without having to restate. Any N-changing impl also
//! satisfies this (it must, to reference `<D as Domain<Self::N2>>`
//! types in its body).

use crate::domain::Domain;

// ANCHOR: lift_trait
/// Domain-generic transformer over the `(treeish, fold)` pair.
///
/// A `Lift` rewrites the graph side and/or the fold side, possibly
/// changing their carrier types, and hands the result to a
/// continuation. The caller's continuation-return type `T` flows
/// through, so the chain of output types stays inferred across
/// composition (`ComposedLift<L1, L2>`).
///
/// Grow is deliberately absent from this signature. Only the Seed
/// finishing lift ([`SeedLift`](super::SeedLift)) needs a grow
/// input; it is composed internally by
/// `hylic_pipeline::PipelineExecSeed::run` and does not travel as
/// a 3-slot signature through the `Lift` trait.
///
/// See [Lifts](https://hylic.balcony.codes/concepts/lifts.html).
pub trait Lift<D, N, H, R>
where D: Domain<N> + Domain<Self::N2>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    /// Output node type after the lift has been applied.
    type N2:   Clone + 'static;
    /// Output heap type after the lift has been applied.
    type MapH: Clone + 'static;
    /// Output result type after the lift has been applied.
    type MapR: Clone + 'static;

    /// Apply the lift to `(treeish, fold)` and invoke `cont` with
    /// the transformed pair.
    fn apply<T>(
        &self,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <D as Domain<Self::N2>>::Graph<Self::N2>,
            <D as Domain<Self::N2>>::Fold<Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T;
}
// ANCHOR_END: lift_trait
