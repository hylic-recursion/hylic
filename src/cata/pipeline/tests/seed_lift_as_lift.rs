//! SeedLift as a first-class library Lift.
//!
//! These tests exercise SeedLift through the public Lift surface
//! (apply / apply_pre_lift) and assert that the explicit path
//! matches `PipelineExec::run`'s internal path.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExec, LiftedNode};
use crate::domain::shared::{self as dom, fold::fold};
use crate::domain::Shared;
use crate::graph::{edgy_visit, Edgy};
use crate::ops::SeedLift;

fn basic_pipeline() -> SeedPipeline<Shared, u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![vec![1, 2], vec![3], vec![], vec![]]);
    let base_fold = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch.get(*n as usize) { for k in kids { cb(k); } }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[test]
fn explicit_seedlift_composition_matches_run() {
    // Implicit path: the convenience `.run_from_slice`.
    let implicit = basic_pipeline()
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    assert_eq!(implicit, 6);

    // Explicit path: compose SeedLift into the chain ourselves and
    // run the resulting pipeline from &LiftedNode::Entry.
    let entry_seeds: Edgy<(), u64> = edgy_visit(move |_: &(), cb: &mut dyn FnMut(&u64)| cb(&0u64));
    let sl: SeedLift<u64, u64, u64> = SeedLift::new(
        |s: &u64| *s,
        entry_seeds,
        || 0u64,
    );
    let explicit: u64 = basic_pipeline()
        .lift()
        .apply_pre_lift(sl)
        .run_from_node(&dom::FUSED, &LiftedNode::Entry);
    assert_eq!(explicit, 6);

    assert_eq!(implicit, explicit);
}

#[test]
fn seed_lift_composes_after_user_shape_lifts() {
    // User's wrap_init applies first; SeedLift applies last.
    // wrap_init adds +100 to each init, so fold sees 100+100+… etc.
    let entry_seeds: Edgy<(), u64> = edgy_visit(move |_: &(), cb: &mut dyn FnMut(&u64)| cb(&0u64));
    let sl: SeedLift<u64, u64, u64> = SeedLift::new(
        |s: &u64| *s,
        entry_seeds,
        || 0u64,
    );
    let result: u64 = basic_pipeline()
        .lift()
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 100)
        .apply_pre_lift(sl)
        .run_from_node(&dom::FUSED, &LiftedNode::Entry);
    // Tree 0 → {1, 2}; 1 → {3}. Per-node init = n + 100.
    // 3 → 103; 1 → 101 + 103 = 204; 2 → 102; 0 → 100 + 204 + 102 = 406.
    // Entry accumulates the root seed result: 0 + 406 = 406.
    assert_eq!(result, 406);
}
