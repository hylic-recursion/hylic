// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! LiftedPipeline source impls.
//!
//! `TreeishSource` impl (always, when Base is TreeishSource): Seed-
//! agnostic. Synthesises a panic-grow internally to satisfy
//! `Lift::apply`'s signature; the panic-grow is never invoked because
//! no Lift impl calls grow at runtime — they compose it only.
//!
//! `SeedSource` impl (when Base is SeedSource): passes the base's
//! grow through the lift chain, preserving Seed-dispatch capability.

use crate::domain::Domain;
use crate::ops::Lift;
use super::LiftedPipeline;
use super::super::source::{TreeishSource, SeedSource};

impl<Base, L> TreeishSource for LiftedPipeline<Base, L>
where Base: TreeishSource,
      <Base as TreeishSource>::Domain: Domain<L::N2>,
      L: Lift<<Base as TreeishSource>::Domain,
              <Base as TreeishSource>::N,
              <Base as TreeishSource>::H,
              <Base as TreeishSource>::R>,
      L::N2:   Clone + 'static,
      L::MapH: Clone + 'static,
      L::MapR: Clone + 'static,
{
    type Domain = <Base as TreeishSource>::Domain;
    type N = L::N2;
    type H = L::MapH;
    type R = L::MapR;

    fn with_treeish<T>(
        &self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T {
        self.base.with_treeish(|treeish, fold| {
            // Synthesise a panic-grow over Seed = (). No Lift impl
            // invokes grow at runtime (N-change lifts compose; they
            // don't call). The panic closure survives through the
            // chain untouched.
            type SeedUnit = ();
            let panic_grow = <<Base as TreeishSource>::Domain as Domain<<Base as TreeishSource>::N>>::make_grow::<SeedUnit, <Base as TreeishSource>::N>(
                |_: &SeedUnit| unreachable!(
                    "panic-grow synthesised for TreeishSource lift-chain application; \
                     no Lift impl should invoke grow at runtime"),
            );
            self.pre_lift.apply::<SeedUnit, _>(panic_grow, treeish, fold,
                |_grow_out, treeish_out, fold_out| cont(treeish_out, fold_out),
            )
        })
    }
}

impl<Base, L> SeedSource for LiftedPipeline<Base, L>
where Base: SeedSource,
      <Base as TreeishSource>::Domain: Domain<L::N2>,
      L: Lift<<Base as TreeishSource>::Domain,
              <Base as TreeishSource>::N,
              <Base as TreeishSource>::H,
              <Base as TreeishSource>::R>,
      L::N2:   Clone + 'static,
      L::MapH: Clone + 'static,
      L::MapR: Clone + 'static,
{
    type Seed = <Base as SeedSource>::Seed;

    fn with_seeded<T>(
        &self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Grow<Self::Seed, Self::N>,
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T {
        self.base.with_seeded(|grow, treeish, fold| {
            self.pre_lift.apply::<<Base as SeedSource>::Seed, _>(grow, treeish, fold, cont)
        })
    }
}
