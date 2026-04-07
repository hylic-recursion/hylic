//! Shared domain — Arc-based storage.
//!
//! The single namespace for composable pipelines, Lift integration,
//! and parallel execution. One import, everything works:
//! ```ignore
//! use hylic::domain::shared as dom;
//! dom::FUSED.run(&dom::fold(...), &dom::treeish_visit(...), &root);
//! ```

use std::marker::PhantomData;
use crate::cata::exec::{fused, sequential, rayon};
use super::Shared;

// ── Executor consts ───────────────────────────────

pub const FUSED:      fused::Exec<Shared>      = fused::Exec(PhantomData);
pub const SEQUENTIAL: sequential::Exec<Shared>  = sequential::Exec(PhantomData);
pub const RAYON:      rayon::Exec<Shared>       = rayon::Exec(PhantomData);

// ── Runtime dispatch ──────────────────────────────

pub use crate::cata::exec::DynExec;

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
