//! Lift<D, N, H, R> — the domain-generic algebra-transform type class.
//!
//! Four trait parameters: D (domain), N (node type), H (heap), R
//! (result). Three associated types (N2, MapH, MapR). One CPS method
//! `apply`. `Seed` is method-level.
//!
//! Under (a-uniform), the closure bounds at grow-construction sites
//! are uniform `Fn + Send + Sync + 'static` across all domains.
//! Per-domain per-closure-type bounds are handled by
//! `FoldTransformsByRef` / `GraphTransformsByRef` (which this trait
//! doesn't bind here — lift bodies import them ad-hoc when wrapping
//! fold or graph closures).

use crate::domain::Domain;

// ANCHOR: lift_trait
pub trait Lift<D, N, H, R>
where D: Domain<N>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    type N2:   Clone + 'static;
    type MapH: Clone + 'static;
    type MapR: Clone + 'static;

    fn apply<Seed, T>(
        &self,
        grow:    D::Grow<Seed, N>,
        treeish: D::Graph<N, N>,
        fold:    D::Fold<H, R>,
        cont: impl FnOnce(
            D::Grow<Seed, Self::N2>,
            D::Graph<Self::N2, Self::N2>,
            D::Fold<Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static;
}
// ANCHOR_END: lift_trait
