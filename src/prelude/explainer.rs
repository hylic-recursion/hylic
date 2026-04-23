//! Explainer data types: `ExplainerStep`, `ExplainerHeap`,
//! `ExplainerResult`, and the `treeish_for_explres` navigator.
//!
//! The `Explainer` *lift* is a constructor on the capable domain:
//! call `Shared::explainer_lift()` or `Local::explainer_lift()`,
//! which produces a `ShapeLift<…>` with the trace-building
//! fold-xform. See `technical-insights/09-unified-shape-lift.md`.
//!
//! Types are parametric over `ChildR` to accommodate both the
//! whole-tree `Explainer` (ChildR = ExplainerResult) and
//! `ExplainerDescribe` (ChildR = R, streaming emit).

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use crate::graph::{treeish, Treeish};

#[derive(Clone)]
pub struct ExplainerStep<H, ChildR>
where H: Clone, ChildR: Clone,
{
    pub incoming_result: ChildR,
    pub resulting_heap:  H,
}

#[derive(Clone)]
pub struct ExplainerHeap<N, H, ChildR>
where N: Clone, H: Clone, ChildR: Clone,
{
    pub initial_heap: H,
    pub node:         N,
    pub transitions:  Vec<ExplainerStep<H, ChildR>>,
    pub working_heap: H,
}

impl<N: Clone, H: Clone, ChildR: Clone> ExplainerHeap<N, H, ChildR> {
    pub fn new(node: N, heap: H) -> Self {
        ExplainerHeap {
            initial_heap: heap.clone(),
            node,
            transitions:  Vec::new(),
            working_heap: heap,
        }
    }
}

/// Whole-tree trace result. Child traces nest recursively via
/// `heap.transitions[i].incoming_result`.
#[derive(Clone)]
pub struct ExplainerResult<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub orig_result: R,
    pub heap:        ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
}

/// Treeish over `ExplainerResult<N, H, R>` — children are the
/// recursive `incoming_result` of each transition. Useful for
/// running a downstream fold over the captured trace.
pub fn treeish_for_explres<N, H, R>() -> Treeish<ExplainerResult<N, H, R>>
where N: Clone + Send + Sync + 'static,
      H: Clone + Send + Sync + 'static,
      R: Clone + Send + Sync + 'static,
{
    treeish(|x: &ExplainerResult<N, H, R>| {
        x.heap.transitions.iter().map(|step| step.incoming_result.clone()).collect()
    })
}
