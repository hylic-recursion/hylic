//! Parallel runtime base: raw primitives and thread infrastructure.
//!
//! - [`pool`]: WorkPool, WakeSignal, worker threads
//! - [`steal_queue`]: monotonic segmented push+steal queue
//! - [`task_slot`]: stack-allocated task with done flag (fork-join)
//! - [`unsafe_core`]: Slot (4-state), Segment (lazy alloc), TaskRef (type-erased)

pub(crate) mod unsafe_core;
pub mod steal_queue;
pub mod task_slot;
pub(crate) mod pool;

pub use pool::{WorkPool, WorkPoolSpec};
