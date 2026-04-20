//! Curated prelude — default case (Shared domain).
//!
//! ```no_run
//! use hylic::prelude::*;
//! ```
//!
//! covers: Shared-domain pipelines, sugars, Fold/Edgy constructors,
//! executor, lift atoms, common debugging helpers (explainer).
//!
//! For Local-domain work, also `use hylic::prelude::local::*;` — the
//! Local version uses non-`Send` storage (Rc / RefCell).
//!
//! For Owned-domain work (one-shot), also `use hylic::prelude::owned::*;`.
//!
//! Advanced helpers (`Traced`, `memoize_treeish`, `VecFold`, etc.)
//! are NOT in the prelude; import them with their explicit paths
//! when needed.

pub mod local;
pub mod owned;

pub(crate) mod utils;
pub mod vec_fold;
pub mod explainer;
pub mod explainer_format;
pub mod format;
pub mod traced;
pub mod memoize;
pub mod common_folds;
pub mod fallible;

// ── Commonly-used helpers (stays as before) ─────────────────────

pub use vec_fold::{vec_fold, VecFold, VecHeap};
pub use explainer::{ExplainerHeap, ExplainerResult, ExplainerStep};
pub use explainer_format::{
    trace_fold_compact, trace_fold_full, trace_fold_brief, trace_fold_indented,
};
pub use format::TreeFormatCfg;
pub use traced::{Traced, traced_treeish};
pub use memoize::{memoize_treeish, memoize_treeish_by};
pub use common_folds::{count_fold, depth_fold, pretty_print};
pub use fallible::seeds_for_fallible;

// ── Domain marker + core traits ─────────────────────────────────

pub use crate::domain::{Domain, Shared};
pub use crate::ops::{FoldOps, TreeOps};
pub use crate::cata::exec::{Exec, Executor};

// ── Shared-domain constructors (default) ────────────────────────

pub use crate::domain::shared::{Fold, fold, simple_fold};
pub use crate::domain::shared::{exec, FUSED};

// ── Graph (Arc-based; matches Shared) ───────────────────────────

pub use crate::graph::{
    Edgy, Treeish,
    edgy, edgy_visit,
    treeish, treeish_visit, treeish_from,
};

// ── Executor variants as modules (opt-in with e.g. `funnel::Spec`) ─

pub use crate::cata::exec::{fused, funnel};

// ── Pipeline types ──────────────────────────────────────────────

pub use crate::cata::pipeline::{
    SeedPipeline, TreeishPipeline, LiftedPipeline, OwnedPipeline,
    TreeishSource, SeedSource, PipelineExec, PipelineExecSeed, PipelineExecOnce,
    LiftedNode,
};

// ── Sugar trait (Shared default) ────────────────────────────────

pub use crate::cata::pipeline::LiftedSugarsShared;

// ── Lift atoms (for power users) ────────────────────────────────

pub use crate::ops::{
    Lift, IdentityLift, ComposedLift, ShapeLift, SeedLift,
    ShapeCapable, PureLift, ShareableLift, LiftBare,
};
