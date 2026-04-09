//! Unsafe foundation for the steal queue infrastructure.
//!
//! This module is the ONLY place in hylic where:
//! - `UnsafeCell<MaybeUninit<T>>` is used (slot.rs)
//! - `AtomicPtr` + `Box::into_raw/from_raw` is used (segment.rs)
//! - `unsafe impl Send/Sync` is asserted (slot.rs)

pub mod slot;
pub mod segment;
