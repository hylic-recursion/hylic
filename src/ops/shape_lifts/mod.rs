//! Shape-lift library — concrete Lift structs that back the pipeline
//! sugar methods. Each file holds one lift: struct + constructor +
//! single `impl Lift<N, Seed, H, R>` body.

pub mod filter_seeds;
pub mod wrap_fold;
pub mod r_transform;

pub use filter_seeds::{FilterSeedsLift, filter_seeds_lift};
pub use wrap_fold::{WrapInitLift, wrap_init_lift};
pub use r_transform::{ZipmapLift, zipmap_lift};
