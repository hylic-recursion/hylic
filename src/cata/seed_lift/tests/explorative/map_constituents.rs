//! Explorative: map_constituents — the general pipeline transform.
//! Tests that all three constituents can be mapped independently.

use std::sync::Arc;
use crate::domain::shared as dom;

#[test]
fn map_constituents_identity() {
    // Identity transform: all three mappers are identity.
    // Result must be the same as the original pipeline.
    let pipeline = super::super::make_pipeline();

    let transformed = pipeline.map_constituents(
        |g| g,    // grow: identity
        |e| e,    // seeds: identity
        |f| f,    // fold: identity
        |l| l,    // pre_lift: identity
    );

    let original = pipeline.run_from_slice(&dom::FUSED, &[0], 0u64);
    let result = transformed.run_from_slice(&dom::FUSED, &[0], 0u64);
    assert_eq!(original, result);
}

#[test]
fn map_constituents_wrap_grow_via_general() {
    // wrap_grow expressed through map_constituents.
    let pipeline = super::super::make_pipeline();

    let transformed = pipeline.map_constituents(
        |g| {
            Arc::new(move |s: &usize| {
                let n = g(s);
                n // could transform here
            })
        },
        |e| e,
        |f| f,
        |l| l,
    );

    let result = transformed.run_from_slice(&dom::FUSED, &[0], 0u64);
    assert_eq!(result, 6); // unchanged
}

#[test]
fn map_constituents_change_fold() {
    // Change fold via map_constituents: multiply result by 10.
    let pipeline = super::super::make_pipeline();

    let transformed = pipeline.map_constituents(
        |g| g,
        |e| e,
        |f| f.map(|r: &u64| *r * 10, |r: &u64| *r / 10),
        |l| l,
    );

    // map(f, f⁻¹): finalize maps r→r*10, accumulate backmaps
    // child r*10→r before folding. Intermediate nodes cancel out.
    // Only the outermost finalize (Entry) applies the *10.
    // Tree sum = 6, Entry accumulates 6, finalizes to 60.
    let result = transformed.run_from_slice(&dom::FUSED, &[0], 0u64);
    assert_eq!(result, 60);
}
