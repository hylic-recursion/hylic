//! Shared domain — Arc-based storage.
//!
//! Clone, Send+Sync. The standard domain for composable pipelines,
//! Lift integration, and parallel execution via Rayon.
//!
//! Types are re-exported from the top-level `fold` and `graph` modules.

pub use crate::fold::{Fold, fold, simple_fold};
pub use crate::fold::{InitFn, AccumulateFn, FinalizeFn};
pub use crate::graph::{
    Treeish, Edgy,
    treeish, treeish_visit, treeish_from,
    edgy, edgy_visit,
    Graph, SeedGraph,
};
