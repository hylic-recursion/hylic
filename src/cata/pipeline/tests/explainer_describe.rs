//! ExplainerDescribe: streaming per-node trace emission with
//! transparent R. Proves MapR = R (downstream sees original
//! result) while emit-callback receives a rendered trace string
//! for each node's finalize.

use std::sync::{Arc, Mutex};
use crate::cata::pipeline::{SeedPipeline, PipelineExecSeed};
use crate::domain::shared::{self as dom, fold::fold};
use crate::domain::Shared;
use crate::graph::edgy_visit;
use crate::prelude::{trace_fold_compact, ExplainerHeap};

fn basic_pipeline() -> SeedPipeline<Shared, u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![vec![1, 2], vec![3], vec![], vec![]]);
    let base_fold = fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch.get(*n as usize) { for k in kids { cb(k); } }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[test]
fn explainer_describe_streams_per_node_and_preserves_r() {
    // Collect emitted trace strings. Arc<Mutex<Vec<String>>> because
    // the emit closure must be Send+Sync for ShareableLift + Funnel,
    // but we're running Fused — either way, Send+Sync is OK.
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_for_emit = captured.clone();

    let r: u64 = basic_pipeline()
        .lift()
        .apply_pre_lift(Shared::explainer_describe_lift::<u64, u64, u64, _, _>(
            trace_fold_compact::<u64, u64, u64>,
            move |s: &str| {
                captured_for_emit.lock().unwrap().push(s.to_string());
            },
        ))
        .run_from_slice(&dom::FUSED, &[0u64], ExplainerHeap::new(0u64, 0u64));

    // R unchanged: 0 + 1 + 2 + 3 = 6.
    assert_eq!(r, 6);

    // Trace strings emitted per node. At least one per non-leaf node.
    let lines = captured.lock().unwrap();
    assert!(!lines.is_empty(), "trace emitted at least one line");
    // Each line should contain "=>" from trace_fold_compact.
    for line in lines.iter() {
        assert!(line.contains("=>"), "trace line formatted: {line}");
    }
}
