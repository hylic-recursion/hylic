//! Shared domain — Arc-based storage.
//!
//! Clone, Send+Sync. The standard domain for composable pipelines,
//! Lift integration, and parallel execution via Rayon.
//!
//! Import as a namespace to get fold constructors, treeish constructors,
//! and compatible executor consts in one place:
//! ```ignore
//! use hylic::domain::shared as dom;
//! dom::FUSED.run(&dom::fold(...), &dom::treeish_visit(...), &root);
//! ```

use std::marker::PhantomData;
use crate::cata::exec::{FusedIn, SequentialIn, RayonIn};
use super::Shared;

// ── Executor consts for this domain ───────────────

pub const FUSED:      FusedIn<Shared>      = FusedIn(PhantomData);
pub const SEQUENTIAL: SequentialIn<Shared>  = SequentialIn(PhantomData);
pub const RAYON:      RayonIn<Shared>       = RayonIn(PhantomData);

// ── Type + constructor re-exports ─────────────────

pub use crate::fold::{Fold, fold, simple_fold};
pub use crate::fold::{InitFn, AccumulateFn, FinalizeFn};
pub use crate::graph::{
    Treeish, Edgy,
    treeish, treeish_visit, treeish_from,
    edgy, edgy_visit,
    Graph, SeedGraph,
};
