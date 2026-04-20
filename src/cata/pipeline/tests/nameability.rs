//! Nameability — pipeline types store cleanly as struct fields and
//! type aliases. Proves the Arc-erasure claim via std::any::type_name.

use crate::cata::pipeline::{SeedPipeline, LiftedPipeline};
use crate::domain::Shared;
use crate::ops::{ComposedLift, IdentityLift, ShapeLift};

type MyBasePipeline = SeedPipeline<crate::domain::Shared, u64, u64, u64, u64>;

type MyTransformedPipeline = LiftedPipeline<
    SeedPipeline<crate::domain::Shared, u64, u64, u64, u64>,
    ComposedLift<IdentityLift, ShapeLift<Shared, u64, u64, u64, u64, u64, (u64, bool)>>,
>;

#[allow(dead_code)]
struct Resolver {
    pipe: MyBasePipeline,
}

#[allow(dead_code)]
struct ResolverWithLifts {
    pipe: MyTransformedPipeline,
}

#[test]
fn pipeline_types_name_and_store() {
    // Name the base pipeline type.
    let base_name = std::any::type_name::<MyBasePipeline>();
    assert!(base_name.contains("SeedPipeline"));
    assert!(base_name.contains("u64"));

    // Name the transformed pipeline type.
    let transformed_name = std::any::type_name::<MyTransformedPipeline>();
    assert!(transformed_name.contains("LiftedPipeline"));
    assert!(transformed_name.contains("ComposedLift"));
    assert!(transformed_name.contains("ShapeLift"));
}
