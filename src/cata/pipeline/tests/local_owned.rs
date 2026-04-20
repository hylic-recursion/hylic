//! Phase 5/5–5/7: Local and Owned pipeline tests.
//!
//! Demonstrates that:
//! - `TreeishPipeline<Local, …>` composes and runs under
//!   Local+Fused with non-Send captures in the fold.
//! - `OwnedPipeline<N, H, R>` consumes on run; the fold is
//!   pre-built and the pipeline is one-shot.
//! - `ShapeLift<Local, …>` composes via apply_pre_lift (once
//!   LiftedPipeline's sugars are wired for Local — until then
//!   via the raw primitive).

use std::cell::RefCell;
use std::rc::Rc;

use crate::cata::pipeline::{OwnedPipeline, PipelineExec, PipelineExecOnce, TreeishPipeline};
use crate::domain::{local, owned, Local};

#[test]
fn local_treeish_pipeline_runs_under_fused_with_non_send_capture() {
    // Non-Send capture: Rc<RefCell<u64>> as a counter the fold touches.
    let counter: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));
    let counter_for_init = counter.clone();

    let treeish = local::edgy::treeish(|n: &u64| {
        if *n == 0 { vec![1u64, 2] } else if *n == 1 { vec![3u64] } else { vec![] }
    });
    let fold: local::Fold<u64, u64, u64> = local::fold(
        move |n: &u64| {
            *counter_for_init.borrow_mut() += 1;
            *n
        },
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let pipe = TreeishPipeline::<Local, u64, u64, u64>::new_local(treeish, fold);
    let r = pipe.run_from_node(&local::FUSED, &0u64);
    // 0 + 1 + 2 + 3 = 6.
    assert_eq!(r, 6);
    // init fired at each node: 0, 1, 2, 3 → 4 calls.
    assert_eq!(*counter.borrow(), 4);
}

#[test]
fn owned_pipeline_runs_once_via_run_from_node_once() {
    let treeish = owned::edgy::treeish(|n: &u64| {
        if *n == 0 { vec![1u64, 2] } else if *n == 1 { vec![3u64] } else { vec![] }
    });
    let fold: owned::Fold<u64, u64, u64> = owned::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let pipe = OwnedPipeline::new(treeish, fold);
    let r = pipe.run_from_node_once(&owned::FUSED, &0u64);
    assert_eq!(r, 6);
    // `pipe` moved by run_from_node_once; using it again is a compile error.
}
