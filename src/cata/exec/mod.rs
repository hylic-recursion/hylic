//! Executor: the strategy for recursive tree computation.
//!
//! Each executor variant is a module with `Exec` and `Spec` types:
//! ```ignore
//! use hylic::cata::exec::fused;
//! fused::Exec::from_spec(fused::Spec).run(&fold, &graph, &root);
//! ```
//!
//! The `Executor` trait exists for generic code (pipeline, advanced users).
//! Normal call sites use inherent methods — no trait import needed.

pub mod variant;

// Module re-exports: each executor variant as a module.
pub use variant::fused;
pub use variant::sequential;
pub use variant::rayon;
pub use variant::pool;
pub use variant::hylomorphic;
pub use variant::hylo_funnel as funnel;
pub use variant::custom;

use std::marker::PhantomData;
use crate::graph::Treeish;
use crate::fold::Fold;
use crate::domain::{Domain, Shared, Local, Owned};
use crate::ops::LiftOps;

// ── Core trait (for generic code only) ────────────

// ANCHOR: executor_trait
pub trait Executor<N: 'static, R: 'static, D: Domain<N>> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R;
}
// ANCHOR_END: executor_trait

// ── Type aliases (convenience for common domain combinations) ──

pub type Fused           = fused::Exec<Shared>;
pub type FusedLocal      = fused::Exec<Local>;
pub type FusedOwned      = fused::Exec<Owned>;
pub type Sequential      = sequential::Exec<Shared>;
pub type SequentialLocal = sequential::Exec<Local>;
pub type SequentialOwned = sequential::Exec<Owned>;
pub type Rayon           = rayon::Exec<Shared>;

// ── Constants ─────────────────────────────────────

pub const FUSED:            Fused           = fused::Exec(PhantomData);
pub const FUSED_LOCAL:      FusedLocal      = fused::Exec(PhantomData);
pub const FUSED_OWNED:      FusedOwned      = fused::Exec(PhantomData);
pub const SEQUENTIAL:       Sequential      = sequential::Exec(PhantomData);
pub const SEQUENTIAL_LOCAL: SequentialLocal = sequential::Exec(PhantomData);
pub const SEQUENTIAL_OWNED: SequentialOwned = sequential::Exec(PhantomData);
pub const RAYON:            Rayon           = rayon::Exec(PhantomData);

// ── DynExec: Shared-domain runtime dispatch ───────

pub enum DynExec<N, R> {
    Fused(Fused),
    Sequential(Sequential),
    Rayon(Rayon),
    Custom(custom::Custom<N, R>),
}

impl<N: 'static, R: 'static> Executor<N, R, Shared> for DynExec<N, R>
where N: Clone + Send + Sync, R: Send + Sync,
{
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        match self {
            Self::Fused(e)      => e.run(fold, graph, root),
            Self::Sequential(e) => e.run(fold, graph, root),
            Self::Rayon(e)      => e.run(fold, graph, root),
            Self::Custom(e)     => <custom::Custom<N, R> as Executor<N, R, Shared>>::run(e, fold, graph, root),
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
