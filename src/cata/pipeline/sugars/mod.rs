// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Blanket sugar traits for Stage-2 transformations.
//!
//! One trait per domain providing default-method sugars on top of
//! the sole primitive `then_lift`. Written once, inherited by every
//! pipeline variant (SeedPipeline, TreeishPipeline, LiftedPipeline).

pub mod lifted_shared;
pub mod lifted_local;

pub use lifted_shared::LiftedSugarsShared;
pub use lifted_local::LiftedSugarsLocal;
