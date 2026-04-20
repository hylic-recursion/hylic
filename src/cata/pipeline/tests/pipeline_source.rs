//! PipelineSource.with_constructed + the blanket PipelineExec.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineSource, PipelineExec};
use crate::domain::shared::{self as dom, fold::fold};
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
fn with_constructed_yields_triple() {
    // The CPS yield. Inspect the triple without running.
    let (inspected_n_val, children_visited) = basic_pipeline()
        .with_constructed(|grow, treeish, _fold| {
            // grow turns seed into node; here identity.
            let n = grow(&0u64);
            // treeish walks N → Vec<N>.
            let mut kids = Vec::new();
            treeish.visit(&n, &mut |c: &u64| kids.push(*c));
            (n, kids)
        });
    assert_eq!(inspected_n_val, 0);
    assert_eq!(children_visited, vec![1, 2]);
}

#[test]
fn with_constructed_delegates_through_lift() {
    // LiftedPipeline's impl delegates to base and threads the lift.
    let r: (u64, bool) = basic_pipeline()
        .lift()
        .zipmap(|r: &u64| *r > 5)
        .with_constructed(|_grow, treeish, fold| {
            // Run the lifted (treeish, fold) directly — skips SeedLift,
            // equivalent to run_from_node.
            dom::FUSED.run(&fold, &treeish, &0u64)
        });
    assert_eq!(r, (6u64, true));
}

#[test]
fn run_from_node_skips_entry() {
    // The run_from_node variant uses with_constructed but bypasses
    // SeedLift, passing the treeish directly to the executor.
    let r = basic_pipeline()
        .lift()
        .run_from_node(&dom::FUSED, &0u64);
    assert_eq!(r, 6);
}

#[test]
fn blanket_run_covers_both_stages() {
    // SeedPipeline's run comes from the same PipelineExec blanket impl
    // as LiftedPipeline's — they are uniform entry points.
    let stage1_result = basic_pipeline().run_from_slice(&dom::FUSED, &[0u64], 0u64);
    let stage2_result = basic_pipeline().lift().run_from_slice(&dom::FUSED, &[0u64], 0u64);
    assert_eq!(stage1_result, stage2_result);
    assert_eq!(stage1_result, 6);
}
