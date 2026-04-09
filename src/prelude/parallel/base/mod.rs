//! Parallel runtime base: steal queue and underlying unsafe primitives.
//!
//! - [`steal_queue`]: monotonic segmented push+steal queue
//! - [`unsafe_core`]: Slot (4-state), Segment (lazy alloc)

pub(crate) mod unsafe_core;
pub mod steal_queue;
