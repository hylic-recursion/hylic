//! SeedLift: anamorphic lift for seed-based graph construction.
//!
//! Organized as:
//! - types: LiftedNode, LiftedHeap
//! - lift: SeedLift (the FP core)
//! - pipeline: SeedPipeline struct + construction
//! - pipeline_transforms: map_constituents and derived transforms
//! - pipeline_run: with_lifted CPS and run methods

pub mod types;
pub mod lift;
pub mod pipeline;
pub mod pipeline_transforms;
pub mod pipeline_run;

#[cfg(test)]
mod tests;

pub use types::{LiftedNode, LiftedHeap};
pub use lift::SeedLift;
pub use pipeline::SeedPipeline;
