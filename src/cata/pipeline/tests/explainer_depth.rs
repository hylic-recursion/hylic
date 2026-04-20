//! T9 (partial): Explainer inserted at multiple chain depths.
//! Same baseline R regardless of where Explainer sits; trace
//! content differs. Also exercises nested Explainer composition.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExecSeed};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::domain::Shared;
use crate::graph::edgy_visit;
use crate::prelude::{ExplainerHeap, ExplainerResult};

fn basic() -> SeedPipeline<crate::domain::Shared, u64, u64, u64, u64> {
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
fn explainer_early_vs_late_same_orig_result() {
    let r_early = basic()
        .lift()
        .then_lift(Shared::explainer_lift::<u64, u64, u64>())
        .zipmap(|r: &ExplainerResult<u64, u64, u64>| r.orig_result * 2)
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], ExplainerHeap::new(0u64, 0u64));
    // Base R = 0+1+2+3 = 6. Zipmap pairs (ExplainerResult{6, …}, 12).
    assert_eq!(r_early.0.orig_result, 6);
    assert_eq!(r_early.1, 12);
    assert!(!r_early.0.heap.transitions.is_empty());

    let r_late: ExplainerResult<u64, u64, (u64, u64)> = basic()
        .lift()
        .zipmap(|r: &u64| r * 2)
        .then_lift(Shared::explainer_lift::<u64, u64, (u64, u64)>())
        .run_from_slice(
            &dom::exec(funnel::Spec::default(4)),
            &[0u64],
            ExplainerHeap::new(0u64, 0u64),
        );
    // R = (u64, u64). Base pair = (6, 12). Explainer wraps.
    assert_eq!(r_late.orig_result, (6, 12));
    assert!(!r_late.heap.transitions.is_empty());
}

#[test]
fn nested_explainers_compose() {
    // Two Explainers in one chain. Inner wraps the base fold; outer
    // wraps the inner-explained fold.
    //   inner MapH = ExplainerHeap<u64, u64, ExplainerResult<u64, u64, u64>>
    //   inner MapR = ExplainerResult<u64, u64, u64>
    //   outer MapH = ExplainerHeap<u64, inner MapH, ExplainerResult<u64, inner MapH, inner MapR>>
    //   outer MapR = ExplainerResult<u64, inner MapH, inner MapR>
    type InnerMapH = ExplainerHeap<u64, u64, ExplainerResult<u64, u64, u64>>;
    type InnerMapR = ExplainerResult<u64, u64, u64>;
    type OuterMapR = ExplainerResult<u64, InnerMapH, InnerMapR>;

    let entry: ExplainerHeap<u64, InnerMapH, OuterMapR> =
        ExplainerHeap::new(0u64, ExplainerHeap::new(0u64, 0u64));

    let r: OuterMapR = basic()
        .lift()
        .then_lift(Shared::explainer_lift::<u64, u64, u64>())
        .then_lift(Shared::explainer_lift::<u64, InnerMapH, InnerMapR>())
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], entry);

    // Unwrap the two trace layers and assert the innermost result.
    assert_eq!(r.orig_result.orig_result, 6);
    assert!(!r.heap.transitions.is_empty(), "outer trace populated");
    assert!(!r.orig_result.heap.transitions.is_empty(), "inner trace populated");
}

#[test]
fn explainer_trace_structure_walks_tree() {
    let r: ExplainerResult<u64, u64, u64> = basic()
        .lift()
        .then_lift(Shared::explainer_lift::<u64, u64, u64>())
        .run_from_slice(
            &dom::exec(funnel::Spec::default(4)),
            &[0u64],
            ExplainerHeap::new(0u64, 0u64),
        );

    assert_eq!(r.heap.transitions.len(), 1, "Entry has one child");

    let zero_step = &r.heap.transitions[0];
    assert_eq!(zero_step.incoming_result.heap.transitions.len(), 2,
               "node 0 has two children (1, 2)");
}
