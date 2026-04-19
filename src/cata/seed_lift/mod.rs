//! SeedLift: anamorphic lift for seed-based graph construction.
//!
//! Module layout:
//! - `types`: LiftedNode, LiftedHeap
//! - `lift`:  SeedLift (the FP core)
//! - `pipeline/`:
//!     - `core`       — SeedPipeline struct + new + map_constituents + drive
//!     - `transforms` — all user-facing transforms (by-value)
//!     - `exec`       — SeedPipelineExec extension trait (run*)

pub mod types;
pub mod lift;
pub mod pipeline;

#[cfg(test)]
mod tests;

pub use types::{LiftedNode, LiftedHeap};
pub use lift::SeedLift;
pub use pipeline::{SeedPipeline, SeedPipelineExec};
