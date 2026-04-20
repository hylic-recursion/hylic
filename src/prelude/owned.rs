//! Owned-domain additions to the default prelude.
//!
//! Owned is not `ShapeCapable`; no Stage-2 sugar surface. Use
//! `OwnedPipeline::new(...).run_from_node_once(...)` for one-shot
//! runs with `Box`-based closure storage.
//!
//! ```no_run
//! use hylic::prelude::*;                      // core types + traits
//! use hylic::prelude::owned::Owned;
//! use hylic::domain::owned::{fold, edgy};
//! ```

pub use crate::domain::Owned;
