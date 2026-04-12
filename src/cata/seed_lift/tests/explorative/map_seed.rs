//! Explorative: map_seed — change the Seed type.
//! Tests bidirectional seed-type transform.

use crate::domain::shared::{self as dom, fold};
use crate::graph;
use crate::cata::seed_lift::pipeline::SeedPipeline;

#[test]
fn map_seed_string_to_usize() {
    // Original: N = usize, Seed = usize.
    // Transform: Seed = usize → String.
    // grow takes String (via from_new: parse), seeds_from_node
    // produces String (via to_new: to_string).
    let pipeline = super::super::make_pipeline();

    let transformed: SeedPipeline<usize, String, u64, u64> = pipeline.map_seed(
        |s: &usize| s.to_string(),          // to_new: usize → String
        |s: &String| s.parse::<usize>().unwrap(), // from_new: String → usize
    );

    // Entry seeds must be String now.
    let result = transformed.run_from_slice(&dom::FUSED, &["0".to_string()], 0u64);
    assert_eq!(result, 6);
}
