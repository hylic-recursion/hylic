//! Explorative: contramap_node — change N to a wrapper type.
//! Tests the bidirectional node-type transform.

use crate::domain::shared::{self as dom, fold};
use crate::graph;
use crate::cata::seed_lift::pipeline::SeedPipeline;

/// A wrapper around usize that adds a label.
#[derive(Clone, Debug)]
struct Labeled { id: usize, label: String }

#[test]
fn contramap_node_wraps_and_unwraps() {
    // Original pipeline: N = usize, Seed = usize
    let pipeline = super::super::make_pipeline();

    // Transform: N = usize → Labeled
    let transformed: SeedPipeline<Labeled, usize, u64, u64> = pipeline.contramap_node(
        |n: &usize| Labeled { id: *n, label: format!("node_{}", n) },
        |l: &Labeled| l.id,
    );

    // The fold sees Labeled nodes (via contra: extracts .id before
    // calling original init). The treeish visits Labeled nodes
    // (via contra: extracts .id, visits original, wraps children as
    // Labeled via co). The grow composes: grow(seed) → usize → Labeled.

    let result = transformed.run_from_slice(&dom::FUSED, &[0], 0u64);
    // Same computation: 0+1+2+3 = 6, plus Entry wrapping.
    assert_eq!(result, 6);
}
