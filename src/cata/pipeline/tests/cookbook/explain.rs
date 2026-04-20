//! Cookbook: explainer_lift / explainer_describe_lift.

use std::sync::{Arc, Mutex};
use crate::cata::pipeline::{SeedPipeline, PipelineExecSeed};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::domain::Shared;
use crate::graph::edgy_visit;
use crate::prelude::{ExplainerHeap, ExplainerResult, trace_fold_compact};

fn basic() -> SeedPipeline<Shared, u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![vec![1, 2], vec![3], vec![], vec![]]);
    let base_fold = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch.get(*n as usize) { for k in kids { cb(k); } }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[test]
fn explainer_lift_records_full_trace() {
    let r: ExplainerResult<u64, u64, u64> = basic()
        .lift()
        .explain()
        .run_from_slice(
            &dom::exec(funnel::Spec::default(4)),
            &[0u64],
            ExplainerHeap::new(0u64, 0u64),
        );
    assert_eq!(r.orig_result, 6);
    assert!(!r.heap.transitions.is_empty(), "trace recorded");
}

#[test]
fn explainer_describe_streams_per_node() {
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_for_emit = captured.clone();

    let r: u64 = basic()
        .lift()
        .then_lift(Shared::explainer_describe_lift::<u64, u64, u64, _, _>(
            trace_fold_compact::<u64, u64, u64>,
            move |s: &str| {
                captured_for_emit.lock().unwrap().push(s.to_string());
            },
        ))
        .run_from_slice(
            &dom::exec(funnel::Spec::default(4)),
            &[0u64],
            ExplainerHeap::new(0u64, 0u64),
        );
    // R is transparent.
    assert_eq!(r, 6);
    assert!(!captured.lock().unwrap().is_empty());
}
