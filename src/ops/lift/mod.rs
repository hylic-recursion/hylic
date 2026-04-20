//! Lift trait and companion types.

pub mod core;
pub mod identity;
pub mod composed;
pub mod shared_domain;
pub mod inline;
pub mod shape;

pub use core::Lift;
pub use identity::IdentityLift;
pub use composed::ComposedLift;
pub use shared_domain::SharedDomainLift;
pub use inline::{InlineLift, inline_lift};
pub use shape::{
    WrapInitLift, wrap_init_lift,
    WrapAccumulateLift, wrap_accumulate_lift,
    WrapFinalizeLift, wrap_finalize_lift,
    ZipmapLift, zipmap_lift,
    MapRLift, map_r_lift,
};
