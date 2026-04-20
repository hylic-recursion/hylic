//! Blanket sugar traits for Stage-2 transformations.
//!
//! One trait per domain providing default-method sugars on top of
//! the sole primitive `then_lift`. Written once, inherited by every
//! pipeline variant (SeedPipeline, TreeishPipeline, LiftedPipeline).

pub mod lifted_shared;

pub use lifted_shared::LiftedSugarsShared;
