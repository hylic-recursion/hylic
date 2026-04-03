//! Executor: the strategy for recursive tree computation.
//!
//! Each executor has inherent `run`, `run_lifted`, `run_lifted_zipped`
//! methods — no trait import needed at call sites:
//! ```ignore
//! use hylic::domain::shared as dom;
//! dom::FUSED.run(&fold, &graph, &root);
//! ```
//!
//! The `Executor` trait exists for generic code (pipeline, advanced users).

pub mod variant;

pub use variant::fused::FusedIn;
pub use variant::sequential::SequentialIn;
pub use variant::rayon::RayonIn;
pub use variant::pool::{PoolIn, PoolSpec};
pub use variant::hylomorphic::{HylomorphicIn, HylomorphicSpec};
pub use variant::custom::Custom;

use std::marker::PhantomData;
use crate::graph::Treeish;
use crate::fold::Fold;
use crate::domain::{Domain, Shared, Local, Owned};
use crate::ops::LiftOps;

// ── Core trait (for generic code only) ────────────

// ANCHOR: executor_trait
/// The executor contract. Used by pipeline.rs and generic functions.
/// Normal call sites use inherent methods instead — no import needed.
pub trait Executor<N: 'static, R: 'static, D: Domain<N>> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R;
}
// ANCHOR_END: executor_trait

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

// ── DynExec: Shared-domain runtime dispatch ───────

pub enum DynExec<N, R> {
    Fused(Fused),
    Sequential(Sequential),
    Rayon(Rayon),
    Custom(Custom<N, R>),
}

impl<N: 'static, R: 'static> Executor<N, R, Shared> for DynExec<N, R>
where N: Clone + Send + Sync, R: Send + Sync,
{
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        match self {
            Self::Fused(e)      => e.run(fold, graph, root),
            Self::Sequential(e) => e.run(fold, graph, root),
            Self::Rayon(e)      => e.run(fold, graph, root),
            Self::Custom(e)     => <Custom<N, R> as Executor<N, R, Shared>>::run(e, fold, graph, root),
        }
    }
}

impl<N: 'static, R: 'static> DynExec<N, R>
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
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }

    pub fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<Shared, N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> (R0, R) where R: Clone {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        let inner = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        (lift.unwrap(inner.clone()), inner)
    }
}

impl<N: 'static, R: 'static> DynExec<N, R> {
    pub fn fused() -> Self { DynExec::Fused(FUSED) }
}

impl<N: Clone + 'static, R: 'static> DynExec<N, R> {
    pub fn sequential() -> Self { DynExec::Sequential(SEQUENTIAL) }
}

impl<N: Clone + Send + Sync + 'static, R: Send + Sync + 'static> DynExec<N, R> {
    pub fn rayon() -> Self { DynExec::Rayon(RAYON) }
}
