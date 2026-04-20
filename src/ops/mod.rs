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
    SeedLift,
};
