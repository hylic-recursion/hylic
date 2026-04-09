//! Shared domain — Arc-based storage.
//!
//! The single namespace for composable pipelines, Lift integration,
//! and parallel execution. One import, everything works:
//! ```ignore
//! use hylic::domain::shared as dom;
//! dom::FUSED.run(&dom::fold(...), &dom::treeish_visit(...), &root);
//! ```

use crate::cata::exec::{Exec, fused};

// ── Executor constants (domain-bound) ────────────

pub const FUSED: Exec<super::Shared, fused::Spec> = Exec::new(fused::Spec);

/// Bind any executor to the Shared domain.
pub const fn exec<S>(s: S) -> Exec<super::Shared, S> { Exec::new(s) }

// ── Fold types + constructors ─────────────────────

pub use crate::fold::{Fold, fold, simple_fold};
pub use crate::fold::{InitFn, AccumulateFn, FinalizeFn};

// ── Graph types + constructors ────────────────────

pub use crate::graph::{
    Treeish, Edgy,
    treeish, treeish_visit, treeish_from,
    edgy, edgy_visit,
    Graph, SeedGraph,
    Visit, visit_slice,
};

// ── Pipeline (Shared-only) ────────────────────────

pub use crate::pipeline::GraphWithFold;
