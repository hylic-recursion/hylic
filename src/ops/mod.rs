pub mod fold;
pub mod tree;
pub mod lift;

pub use fold::{FoldOps, FoldTransformsByRef, FoldTransformsByValue};
pub use tree::{TreeOps, GraphTransformsByRef, GraphTransformsByValue};
pub use lift::{
    Lift,
    IdentityLift,
    ComposedLift,
    ShapeLift,
    ShapeCapable,
    PureLift,
    ShareableLift,
    LiftBare,
    SeedLift,
    LiftedNode,
};

// Doc-hidden passthrough for `hylic-pipeline`'s internal Node/Entry
// dispatch. Not part of the stable surface.
#[doc(hidden)]
pub use lift::lifted_node_internal;
