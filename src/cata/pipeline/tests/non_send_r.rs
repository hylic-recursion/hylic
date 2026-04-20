//! Non-Send R under sequential executors — `run_from_node` accepts
//! it; `run` / `run_from_slice` do not (they require Send+Sync).

use std::rc::Rc;
use crate::cata::pipeline::{TreeishPipeline, PipelineExec};
use crate::domain::shared::{self as dom, fold::fold};
use crate::graph::treeish;

#[derive(Clone)]
struct Node { val: u64, children: Vec<Node> }

/// Rc<T> is not Send. We build a Fold with R = Rc<u64>.
#[test]
fn non_send_r_runs_via_run_from_node() {
    // Fold produces Rc<u64>. (The fold storage itself is Send+Sync
    // via Arc<dyn Fn... + Send + Sync>; R = Rc<u64> is Clone + 'static.)
    let f = fold(
        |n: &Node| n.val,
        |h: &mut u64, c: &Rc<u64>| *h += **c,
        |h: &u64| Rc::new(*h),
    );
    let g = treeish(|n: &Node| n.children.clone());
    let root = Node { val: 1, children: vec![
        Node { val: 2, children: vec![] },
        Node { val: 3, children: vec![] },
    ]};

    // run_from_node: no Send bounds on R required.
    let r = TreeishPipeline::new(g, &f).run_from_node(&dom::FUSED, &root);
    assert_eq!(*r, 6);
}

// Note: `pipeline.run(&dom::FUSED, ...)` with R = Rc<u64> is a
// compile error in the Phase-4 bound split, because `run` has
// `Self::H: Send + Sync` (needed for the entry_heap move-closure).
// Rc<u64> would still pass the H: Send + Sync check here since H =
// u64. The actual block on `run` with non-Send R comes from the
// Funnel executor's internal bounds, not from the trait. This test
// documents the design; the counterexample for bound-failure would
// live as a trybuild test (out of scope for this file).
