//! Shape-lift library — concrete Lift structs that back the pipeline
//! sugar methods. Each file holds one lift: struct + constructor +
//! single `impl Lift<N, Seed, H, R>` body.

pub mod filter_seeds;
pub mod wrap_fold;
pub mod r_transform;
pub mod type_change;

pub use filter_seeds::{FilterSeedsLift, filter_seeds_lift};
pub use wrap_fold::{
    WrapInitLift, wrap_init_lift,
    WrapAccumulateLift, wrap_accumulate_lift,
    WrapFinalizeLift, wrap_finalize_lift,
    WrapGrowLift, wrap_grow_lift,
};
pub use r_transform::{
    ZipmapLift, zipmap_lift,
    MapRLift, map_r_lift,
};
pub use type_change::{
    ContramapNodeLift, contramap_node_lift,
    MapSeedLift, map_seed_lift,
};
