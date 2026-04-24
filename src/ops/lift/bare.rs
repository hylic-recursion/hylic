//! `LiftBare` — apply any Lift to a bare `(treeish, fold)` pair.
//!
//! Users who don't use pipelines can still construct library Lifts
//! via `Shared::foo_lift(args)` / `Local::foo_lift(args)` and apply
//! them directly to a treeish+fold, skipping the pipeline machinery.

use crate::exec::Executor;
use crate::domain::Domain;
use crate::ops::TreeOps;
use super::capability::ShapeCapable;
use super::core::Lift;

// ANCHOR: lift_bare_trait
/// Blanket trait extending any [`Lift`] with direct application to
/// a bare `(treeish, fold)` pair. Implemented automatically; users
/// call `.apply_bare(...)` or `.run_on(...)` without a pipeline.
pub trait LiftBare<D, N, H, R>: Lift<D, N, H, R>
where D: ShapeCapable<N> + Domain<Self::N2>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
      Self::N2:   Clone + 'static,
      Self::MapH: Clone + 'static,
      Self::MapR: Clone + 'static,
{
    /// Apply this lift to a bare (treeish, fold) pair; return the
    /// transformed pair.
    fn apply_bare(
        &self,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
    ) -> (<D as Domain<Self::N2>>::Graph<Self::N2>,
          <D as Domain<Self::N2>>::Fold<Self::MapH, Self::MapR>)
    {
        self.apply(treeish, fold, |t, f| (t, f))
    }

    /// Apply this lift and run the result under the given executor.
    fn run_on<E>(
        &self,
        exec:    &E,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
        root:    &Self::N2,
    ) -> Self::MapR
    where
        E: Executor<
            Self::N2, Self::MapR, D,
            <D as Domain<Self::N2>>::Graph<Self::N2>,
        >,
        <D as Domain<Self::N2>>::Graph<Self::N2>: TreeOps<Self::N2>,
    {
        let (t, f) = self.apply_bare(treeish, fold);
        exec.run(&f, &t, root)
    }
}
// ANCHOR_END: lift_bare_trait

impl<L, D, N, H, R> LiftBare<D, N, H, R> for L
where L: Lift<D, N, H, R>,
      D: ShapeCapable<N> + Domain<L::N2>,
      N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
      L::N2:   Clone + 'static,
      L::MapH: Clone + 'static,
      L::MapR: Clone + 'static,
{}
