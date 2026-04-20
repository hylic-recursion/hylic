//! Phase-3 pipeline — two typestates sharing a CPS yield primitive.
//!
//! - `SeedPipeline<N, Seed, H, R>` (Stage 1, coalgebra) — holds the
//!   three base slots. Sole primitive: `reshape`. Sugars:
//!   `filter_seeds`, `wrap_grow`, `contramap_node`, `map_seed`.
//!   Transition to Stage 2 via `.lift()`.
//!
//! - `LiftedPipeline<N, Seed, H, R, L = IdentityLift>` (Stage 2,
//!   algebra) — holds a Stage-1 base plus a lift chain. Sole
//!   primitive: `apply_pre_lift`. Sugars: `wrap_init`,
//!   `wrap_accumulate`, `wrap_finalize`, `zipmap`, `map`.
//!
//! Both typestates implement `PipelineSource` (sole primitive
//! `with_constructed(cont)` yielding `(grow, treeish, fold)` via CPS)
//! and transitively `PipelineExec` (blanket-implemented `run`,
//! `run_from_slice`, `run_from_node`).

pub mod source;
pub mod seed;
pub mod treeish;
pub mod lifted;
pub mod owned;
pub(crate) mod internal;

#[cfg(test)]
mod tests;

pub use source::{PipelineSource, PipelineSourceOnce, PipelineExec, PipelineExecOnce};
pub use seed::SeedPipeline;
pub use treeish::TreeishPipeline;
pub use lifted::LiftedPipeline;
pub use owned::OwnedPipeline;
pub use internal::lifted_types::{LiftedNode, LiftedHeap};
