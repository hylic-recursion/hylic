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
pub trait Lift<D, N, H, R>
where D: Domain<N> + Domain<Self::N2>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2:   Clone + 'static;
    type MapH: Clone + 'static;
    type MapR: Clone + 'static;

    fn apply<Seed, T>(
        &self,
        grow:    <D as Domain<N>>::Grow<Seed, N>,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <D as Domain<Self::N2>>::Grow<Seed, Self::N2>,
            <D as Domain<Self::N2>>::Graph<Self::N2>,
            <D as Domain<Self::N2>>::Fold<Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static;
}
// ANCHOR_END: lift_trait
