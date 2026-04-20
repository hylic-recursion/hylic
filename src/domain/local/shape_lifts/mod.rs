// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Local-domain shape-lift catalogue. Mirror of Shared with Rc
//! storage and non-Send bounds.

pub mod primitives;
pub mod fold_sugars;
pub mod treeish_sugars;
pub mod n_change_sugars;
pub mod explainer;
