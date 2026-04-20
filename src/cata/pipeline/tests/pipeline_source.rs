//! TreeishSource.with_treeish + SeedSource.with_seeded + blanket
//! PipelineExec / PipelineExecSeed.

use std::sync::Arc;
use crate::cata::pipeline::{
    SeedPipeline, TreeishSource, SeedSource, PipelineExec, PipelineExecSeed,
    LiftedSugarsShared,
};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::edgy_visit;

fn basic_pipeline() -> SeedPipeline<crate::domain::Shared, u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![vec![1, 2], vec![3], vec![], vec![]]);
    let base_fold = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch.get(*n as usize) {
            for k in kids { cb(k); }
        }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[test]
fn with_seeded_yields_triple() {
    // SeedSource's 3-slot yield. Inspect grow, treeish, fold.
    let (inspected_n_val, children_visited) = basic_pipeline()
        .with_seeded(|grow, treeish, _fold| {
            let n = grow(&0u64);
            let mut kids = Vec::new();
            treeish.visit(&n, &mut |c: &u64| kids.push(*c));
            (n, kids)
        });
    assert_eq!(inspected_n_val, 0);
    assert_eq!(children_visited, vec![1, 2]);
}

#[test]
fn with_treeish_yields_pair() {
    // TreeishSource's 2-slot yield. Seed-agnostic; grow is not exposed.
    let r: u64 = basic_pipeline()
        .with_treeish(|treeish, fold| {
            dom::FUSED.run(&fold, &treeish, &0u64)
        });
    assert_eq!(r, 6);
}

#[test]
fn with_treeish_delegates_through_lift() {
    // LiftedPipeline's TreeishSource impl synthesises a panic-grow
    // to feed the lift chain, then discards it. Seed-agnostic path.
    let r: (u64, bool) = basic_pipeline()
        .lift()
        .zipmap(|r: &u64| *r > 5)
        .with_treeish(|treeish, fold| {
            dom::FUSED.run(&fold, &treeish, &0u64)
        });
    assert_eq!(r, (6u64, true));
}

#[test]
fn run_from_node_skips_entry() {
    // PipelineExec (blanket on TreeishSource) bypasses SeedLift.
    let r = basic_pipeline()
        .lift()
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &0u64);
    assert_eq!(r, 6);
}

#[test]
fn run_from_slice_covers_both_stages() {
    // PipelineExecSeed (blanket on SeedSource) handles Entry dispatch.
    let stage1_result = basic_pipeline().run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    let stage2_result = basic_pipeline().lift().run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    assert_eq!(stage1_result, stage2_result);
    assert_eq!(stage1_result, 6);
}
