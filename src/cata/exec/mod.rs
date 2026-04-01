//! Executor: the strategy for recursive tree computation.
//!
//! Four built-in variants, each in its own module under `variant/`:
//!
//! | Variant | Traversal | Parallelism | Size | Arc/node |
//! |---------|-----------|-------------|------|----------|
//! | [`Fused`] | callback | none | 0 | 0 |
//! | [`Sequential`] | Vec collect | none | 0 | 0 |
//! | [`Rayon`] | Vec collect | rayon par_iter | 0 | 0 |
//! | [`Custom`] | user-defined | user-defined | Arc | 5 |
//!
//! All implement the [`Executor`] trait. The [`Exec`] enum wraps them
//! for runtime dispatch with an unchanged public API.
//!
//! For zero-overhead static dispatch, use variant types directly:
//! ```ignore
//! Fused.run(&fold, &graph, &root)
//! Rayon.run(&fold, &graph, &root)
//! ```

pub mod variant;

pub use variant::fused::Fused;
pub use variant::sequential::Sequential;
pub use variant::rayon::Rayon;
pub use variant::custom::{Custom, ChildVisitorFn};

use crate::graph::Treeish;
use crate::fold::Fold;
use super::Lift;

// ── Trait ──────────────────────────────────────────

/// The executor contract. Implementors define `run` — the recursive
/// traversal strategy. Lift integration is provided automatically.
pub trait Executor<N: 'static, R: 'static> {
    /// Execute a fold over a tree rooted at `root`.
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R;

    // ANCHOR: run_lifted
    /// Lift fold + graph into a different type domain, execute, unwrap.
    fn run_lifted<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &Lift<N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> R0 {
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }
    // ANCHOR_END: run_lifted

    /// Like `run_lifted`, but also returns the inner (lifted-domain) result.
    fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &Lift<N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> (R0, R) where R: Clone {
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_root = lift.lift_root(root);
        let inner = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        (lift.unwrap(inner.clone()), inner)
    }
}

// ── Exec enum ─────────────────────────────────────

/// Runtime-polymorphic executor wrapping all built-in variants.
///
/// Use `Exec::fused()`, `Exec::rayon()`, etc. — the API is unchanged.
/// For zero-overhead static dispatch, use `Fused`, `Rayon`, etc. directly.
pub enum Exec<N, R> {
    Fused(Fused),
    Sequential(Sequential),
    Rayon(Rayon),
    Custom(Custom<N, R>),
}

impl<N: 'static, R: 'static> Executor<N, R> for Exec<N, R>
where
    N: Clone + Send + Sync,
    R: Send + Sync,
{
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        match self {
            Self::Fused(e)      => e.run(fold, graph, root),
            Self::Sequential(e) => e.run(fold, graph, root),
            Self::Rayon(e)      => e.run(fold, graph, root),
            Self::Custom(e)     => e.run(fold, graph, root),
        }
    }
}

// ── Inherent methods: usable without importing Executor ──

impl<N: 'static, R: 'static> Exec<N, R>
where
    N: Clone + Send + Sync,
    R: Send + Sync,
{
    pub fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        match self {
            Self::Fused(e)      => e.run(fold, graph, root),
            Self::Sequential(e) => e.run(fold, graph, root),
            Self::Rayon(e)      => e.run(fold, graph, root),
            Self::Custom(e)     => e.run(fold, graph, root),
        }
    }

    pub fn run_lifted<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &Lift<N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> R0 {
        <Self as Executor<N, R>>::run_lifted(self, lift, fold, graph, root)
    }

    pub fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &Lift<N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> (R0, R) where R: Clone {
        <Self as Executor<N, R>>::run_lifted_zipped(self, lift, fold, graph, root)
    }
}

// ── Convenience constructors ──────────────────────

impl<N: 'static, R: 'static> Exec<N, R> {
    pub fn fused() -> Self { Exec::Fused(Fused) }
}

impl<N: Clone + 'static, R: 'static> Exec<N, R> {
    pub fn sequential() -> Self { Exec::Sequential(Sequential) }
}

impl<N: Clone + Send + Sync + 'static, R: Send + Sync + 'static> Exec<N, R> {
    pub fn rayon() -> Self { Exec::Rayon(Rayon) }
}

// ── From conversions ──────────────────────────────

impl<N, R> From<Fused> for Exec<N, R> {
    fn from(e: Fused) -> Self { Exec::Fused(e) }
}

impl<N, R> From<Sequential> for Exec<N, R> {
    fn from(e: Sequential) -> Self { Exec::Sequential(e) }
}

impl<N, R> From<Rayon> for Exec<N, R> {
    fn from(e: Rayon) -> Self { Exec::Rayon(e) }
}

impl<N, R> From<Custom<N, R>> for Exec<N, R> {
    fn from(e: Custom<N, R>) -> Self { Exec::Custom(e) }
}
