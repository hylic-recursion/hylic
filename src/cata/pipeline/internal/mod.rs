//! Internal machinery: the lifted types (`LiftedNode`, `LiftedHeap`)
//! that SeedLift uses. SeedLift itself lives in `ops::lift::seed_lift`
//! as a library Lift.
//!
//! `LiftedNode` and `LiftedHeap` are pub — they appear in
//! user-visible executor bounds (`Executor<LiftedNode<Seed, N>, R, …>`)
//! and in `SeedLift`'s associated types.

pub mod lifted_types;

pub use lifted_types::LiftedNode;
