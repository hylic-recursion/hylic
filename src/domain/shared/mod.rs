//! Shared domain — Arc-based storage.
//!
//! Clone, Send+Sync. The single namespace for composable pipelines,
//! Lift integration, and parallel execution.
//!
//! ```ignore
//! use hylic::domain::shared::{self, Executor};
//! shared::FUSED.run(&shared::fold(...), &shared::treeish_visit(...), &root);
//! ```

use std::marker::PhantomData;
use crate::cata::exec::{FusedIn, SequentialIn, RayonIn};
use super::Shared;

// ── Executor consts ───────────────────────────────

pub const FUSED:      FusedIn<Shared>      = FusedIn(PhantomData);
pub const SEQUENTIAL: SequentialIn<Shared>  = SequentialIn(PhantomData);
pub const RAYON:      RayonIn<Shared>       = RayonIn(PhantomData);

// ── Traits (import for .run() / .run_lifted()) ────

pub use crate::cata::exec::Executor;
pub use crate::cata::exec::ExecutorExt;

// ── Fold types + constructors ─────────────────────

pub use crate::fold::{Fold, fold, simple_fold};
pub use crate::fold::{InitFn, AccumulateFn, FinalizeFn};

// ── Graph types + constructors ────────────────────

pub use crate::graph::{
    Treeish, Edgy,
    treeish, treeish_visit, treeish_from,
    edgy, edgy_visit,
    Graph, SeedGraph,
};

// ── Pipeline (Shared-only) ────────────────────────

pub use crate::pipeline::GraphWithFold;
