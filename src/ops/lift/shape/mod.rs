//! Shape-lift catalogue — concrete Lift structs backing LiftedPipeline
//! algebra sugars. Five structs, two files. Each struct: manual Clone
//! + constructor fn + one-line apply body.

pub mod wrap_fold;
pub mod r_transform;

pub use wrap_fold::{
    WrapInitLift, wrap_init_lift,
    WrapAccumulateLift, wrap_accumulate_lift,
    WrapFinalizeLift, wrap_finalize_lift,
};
pub use r_transform::{
    ZipmapLift, zipmap_lift,
    MapRLift, map_r_lift,
};
