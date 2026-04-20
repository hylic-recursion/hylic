// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Shared-domain shape-lift catalogue.
//!
//! Three primitives (primitives.rs) underpin every sugar.
//! Sugars are one-line wrappers; see the category files.

pub mod primitives;
pub mod fold_sugars;
pub mod treeish_sugars;
pub mod n_change_sugars;
pub mod explainer;
