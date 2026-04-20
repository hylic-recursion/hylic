// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Internal machinery: the lifted-node type that SeedLift uses.
//! `LiftedNode` is pub — it appears in user-visible executor bounds
//! (`Executor<LiftedNode<N>, R, …>`) and in `SeedLift`'s associated
//! types.

pub mod lifted_types;

pub use lifted_types::LiftedNode;
