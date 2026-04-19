//! SeedPipeline: the user-facing wrapper over SeedLift.
//!
//! Layered architecture:
//!
//! - `core`: the two primitives.
//!     - `SeedPipeline` struct + `new`
//!     - `map_constituents` — sole type-reshape primitive (by value)
//!     - `drive` — sole CPS execution primitive
//!
//! - `transforms`: every convenience transform, each a one-line
//!   wrapper over `map_constituents`. By-value (consumes self).
//!
//! - `exec`: `SeedPipelineExec` extension trait carrying the
//!   executor-imposed bounds. Re-exported from the prelude.

pub mod core;
pub mod transforms;
pub mod exec;

pub use core::SeedPipeline;
pub use exec::SeedPipelineExec;
