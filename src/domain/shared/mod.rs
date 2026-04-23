//! Shared domain — Arc-based fold storage.
//!
//! The Shared domain provides Clone + Send + Sync folds, enabling
//! parallel execution (Funnel) and lift integration (Explainer).
//! Graph types are domain-independent — see `hylic::graph`.
//!
//! ```ignore
//! use hylic::domain::shared as dom;
//! use hylic::graph;
//! dom::FUSED.run(&dom::fold(...), &graph::treeish_visit(...), &root);
//! ```

#![allow(missing_docs)] // implementation surface; items documented at the trait/type they implement

pub mod fold;
pub mod shape_capable;
pub mod shape_lifts;

use crate::exec::{Exec, fused};

pub const FUSED: Exec<super::Shared, fused::Spec> = Exec::new(fused::Spec);

/// Bind any executor to the Shared domain.
pub const fn exec<S>(s: S) -> Exec<super::Shared, S> { Exec::new(s) }

pub use fold::{Fold, fold};
