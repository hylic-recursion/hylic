//! Internal machinery: SeedLift (Entry/Seed/Node dispatch) and the
//! lifted types (LiftedNode, LiftedHeap).
//!
//! SeedLift is pub(crate) — user code never constructs one directly;
//! it lives inside PipelineExec::run as post-composition.
//!
//! LiftedNode and LiftedHeap are pub — they appear in user-visible
//! executor bounds (Executor<LiftedNode<Seed, N>, R, ...>).

pub mod lifted_types;
pub(crate) mod seed_lift;

pub use lifted_types::LiftedNode;
pub(crate) use seed_lift::SeedLift;
