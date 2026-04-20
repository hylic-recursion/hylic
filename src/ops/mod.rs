pub mod fold;
pub mod tree;
pub mod lift;

pub use fold::FoldOps;
pub use tree::TreeOps;
pub use lift::{
    Lift,
    IdentityLift,
    ComposedLift,
    SharedDomainLift,
    InlineLift, inline_lift,
    WrapInitLift, wrap_init_lift,
    WrapAccumulateLift, wrap_accumulate_lift,
    WrapFinalizeLift, wrap_finalize_lift,
    ZipmapLift, zipmap_lift,
    MapRLift, map_r_lift,
};
