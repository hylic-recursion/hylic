//! Executor: the strategy for recursive tree computation.
//!
//! ```ignore
//! use hylic::cata::exec::{self, Executor};
//! exec::FUSED.run(&fold, &graph, &root);
//! exec::RAYON.run(&fold, &graph, &root);
//! ```

pub mod variant;

pub use variant::fused::FusedIn;
pub use variant::sequential::SequentialIn;
pub use variant::rayon::RayonIn;
pub use variant::pool::{PoolIn, PoolSpec};
pub use variant::custom::Custom;

use std::marker::PhantomData;
use crate::graph::Treeish;
use crate::fold::Fold;
use crate::domain::{Domain, Shared, Local, Owned};
use crate::ops::LiftOps;

// ── Core trait ────────────────────────────────────

// ANCHOR: executor_trait
pub trait Executor<N: 'static, R: 'static, D: Domain<N>> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R;
}
// ANCHOR_END: executor_trait

// ── Lift extension (domain-generic) ──────────────

// ANCHOR: executor_ext_trait
pub trait ExecutorExt<N: 'static, R: 'static, D: Domain<N>>: Executor<N, R, D> {
    // ANCHOR: run_lifted
    fn run_lifted<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> R0
    where
        D: Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }
    // ANCHOR_END: run_lifted

    fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> (R0, R)
    where
        D: Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
        R: Clone,
    {
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_root = lift.lift_root(root);
        let inner = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        (lift.unwrap(inner.clone()), inner)
    }
}
// ANCHOR_END: executor_ext_trait

impl<N: 'static, R: 'static, D: Domain<N>, E: Executor<N, R, D>> ExecutorExt<N, R, D> for E {}

// ── Type aliases ──────────────────────────────────

pub type Fused           = FusedIn<Shared>;
pub type FusedLocal      = FusedIn<Local>;
pub type FusedOwned      = FusedIn<Owned>;
pub type Sequential      = SequentialIn<Shared>;
pub type SequentialLocal = SequentialIn<Local>;
pub type SequentialOwned = SequentialIn<Owned>;
pub type Rayon           = RayonIn<Shared>;

// ── Constants ─────────────────────────────────────

pub const FUSED:            Fused           = FusedIn(PhantomData);
pub const FUSED_LOCAL:      FusedLocal      = FusedIn(PhantomData);
pub const FUSED_OWNED:      FusedOwned      = FusedIn(PhantomData);
pub const SEQUENTIAL:       Sequential      = SequentialIn(PhantomData);
pub const SEQUENTIAL_LOCAL: SequentialLocal = SequentialIn(PhantomData);
pub const SEQUENTIAL_OWNED: SequentialOwned = SequentialIn(PhantomData);
pub const RAYON:            Rayon           = RayonIn(PhantomData);

// ── Exec enum: Shared-domain runtime dispatch ─────

// ANCHOR: exec_enum
pub enum Exec<N, R> {
    Fused(Fused),
    Sequential(Sequential),
    Rayon(Rayon),
    Custom(Custom<N, R>),
}
// ANCHOR_END: exec_enum

impl<N: 'static, R: 'static> Executor<N, R, Shared> for Exec<N, R>
where N: Clone + Send + Sync, R: Send + Sync,
{
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        match self {
            Self::Fused(e)      => <Fused as Executor<N, R, Shared>>::run(e, fold, graph, root),
            Self::Sequential(e) => <Sequential as Executor<N, R, Shared>>::run(e, fold, graph, root),
            Self::Rayon(e)      => <Rayon as Executor<N, R, Shared>>::run(e, fold, graph, root),
            Self::Custom(e)     => <Custom<N, R> as Executor<N, R, Shared>>::run(e, fold, graph, root),
        }
    }
}

impl<N: 'static, R: 'static> Exec<N, R>
where N: Clone + Send + Sync, R: Send + Sync,
{
    pub fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        <Self as Executor<N, R, Shared>>::run(self, fold, graph, root)
    }

    pub fn run_lifted<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<Shared, N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> R0 {
        <Self as ExecutorExt<N, R, Shared>>::run_lifted(self, lift, fold, graph, root)
    }

    pub fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<Shared, N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> (R0, R) where R: Clone {
        <Self as ExecutorExt<N, R, Shared>>::run_lifted_zipped(self, lift, fold, graph, root)
    }
}

// ── Exec constructors ─────────────────────────────

impl<N: 'static, R: 'static> Exec<N, R> {
    pub fn fused() -> Self { Exec::Fused(FUSED) }
}

impl<N: Clone + 'static, R: 'static> Exec<N, R> {
    pub fn sequential() -> Self { Exec::Sequential(SEQUENTIAL) }
}

impl<N: Clone + Send + Sync + 'static, R: Send + Sync + 'static> Exec<N, R> {
    pub fn rayon() -> Self { Exec::Rayon(RAYON) }
}
