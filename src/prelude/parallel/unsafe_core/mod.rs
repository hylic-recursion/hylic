//! Unsafe foundation for the parallel execution infrastructure.
//!
//! This module is the ONLY place in hylic where:
//! - `UnsafeCell<MaybeUninit<T>>` is used (slot.rs)
//! - `AtomicPtr` + `Box::into_raw/from_raw` is used (segment.rs)
//! - Raw function pointers + `*const ()` appear (task_ref.rs)
//! - `unsafe impl Send/Sync` is asserted (slot.rs, task_ref.rs)
//!
//! Everything above this module (steal_queue, task_slot, pool, lifts)
//! uses safe Rust only, building on the safe APIs exported here.

pub mod slot;
pub mod segment;
pub mod task_ref;
