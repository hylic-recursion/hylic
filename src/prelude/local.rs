//! Local-domain additions to the core prelude.
//!
//! ```no_run
//! use hylic::prelude::*;
//! use hylic::prelude::local::Local;
//! use hylic::domain::local::{fold, edgy};
//! ```
//!
//! For the Local sugar trait on pipelines, use
//! `use hylic_pipeline::LiftedSugarsLocal;` — it lives in the
//! pipeline crate.

pub use crate::domain::Local;
