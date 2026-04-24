//! hylic core prelude.
//!
//! ```no_run
//! use hylic::prelude::*;
//! ```
//!
//! covers: the three domain markers (Shared / Local / Owned),
//! Shared-default Fold/Edgy/Treeish constructors, executor helpers,
//! every lift atom (Lift, IdentityLift, ComposedLift, ShapeLift,
//! SeedLift, LiftBare, capability markers, LiftedNode), and common
//! debug helpers (explainer, formatters).
//!
//! For Local or Owned construction, import the per-domain module
//! directly: `use hylic::domain::local as ldom;` then use
//! `ldom::fold(…)`, `ldom::edgy::treeish(…)`, `ldom::FUSED`, etc.
//!
//! For pipeline typestates + sugar traits, depend on
//! `hylic-pipeline` and use `use hylic_pipeline::prelude::*;`
//! (which re-exports everything below, plus pipelines).
//!
//! Advanced helpers (`Traced`, `memoize_treeish`, `VecFold`, etc.)
//! are NOT in the prelude; import them with their explicit paths.

pub(crate) mod utils;
pub mod vec_fold;
pub mod explainer;
pub mod explainer_format;
pub mod format;
pub mod traced;
pub mod memoize;
pub mod common_folds;
pub mod fallible;

// ── Commonly-used helpers ───────────────────────────────────────

pub use vec_fold::{vec_fold, VecFold, VecHeap};
pub use explainer::{ExplainerHeap, ExplainerResult, ExplainerStep, SeedExplainerResult};
pub use explainer_format::{
    trace_fold_compact, trace_fold_full, trace_fold_brief, trace_fold_indented,
};
pub use format::TreeFormatCfg;
pub use traced::{Traced, traced_treeish};
pub use memoize::{memoize_treeish, memoize_treeish_by};
pub use common_folds::{count_fold, depth_fold, pretty_print};
pub use fallible::seeds_for_fallible;

// ── Domain markers + core traits ────────────────────────────────

pub use crate::domain::{Domain, Shared, Local, Owned};
pub use crate::ops::{FoldOps, TreeOps};
pub use crate::exec::{Exec, Executor};

// ── Shared-domain constructors (default) ────────────────────────

pub use crate::domain::shared::{Fold, fold};
pub use crate::domain::shared::{exec, FUSED};

// ── Graph (Arc-based; matches Shared) ───────────────────────────

pub use crate::graph::{
    Edgy, Treeish,
    edgy, edgy_visit,
    treeish, treeish_visit, treeish_from,
};

// ── Executor variants as modules (opt-in with e.g. `funnel::Spec`) ─

pub use crate::exec::{fused, funnel};

// ── Lift atoms (including SeedLift, LiftBare, LiftedNode) ───────

pub use crate::ops::{
    Lift, IdentityLift, ComposedLift, ShapeLift, SeedLift, LiftedNode,
    ShapeCapable, PureLift, ShareableLift, LiftBare,
};
