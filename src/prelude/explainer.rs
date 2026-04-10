//! Explainer: computation tracing as a Lift.
//!
//! Wraps a fold to record the full computation trace at every node —
//! initial heap, each child result accumulated, and the final result.
//! This is a histomorphism: each node sees its subtree's full history.
//!
//! Usage:
//!   lift::run_lifted(&dom::FUSED, &Explainer, &fold, &graph, &root)
//!   lift::run_lifted_zipped(&dom::FUSED, &Explainer, &fold, &graph, &root)

use crate::graph::{treeish, Treeish};
use crate::domain::shared::fold::Fold;
use crate::ops::LiftOps;

// ── Trace data types ───────────────────────────────────────

#[derive(Clone)]
pub struct ExplainerStep<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub incoming_result: ExplainerResult<N, H, R>,
    pub resulting_heap: H,
}

#[derive(Clone)]
pub struct ExplainerHeap<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub initial_heap: H,
    pub node: N,
    pub transitions: Vec<ExplainerStep<N, H, R>>,
    pub working_heap: H,
}

impl<N: Clone, H: Clone, R: Clone> ExplainerHeap<N, H, R> {
    fn new(node: N, heap: H) -> Self {
        ExplainerHeap {
            initial_heap: heap.clone(),
            node,
            transitions: Vec::new(),
            working_heap: heap,
        }
    }
}

#[derive(Clone)]
pub struct ExplainerResult<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub orig_result: R,
    pub heap: ExplainerHeap<N, H, R>,
}

type EH<N, H, R> = ExplainerHeap<N, H, R>;
type ER<N, H, R> = ExplainerResult<N, H, R>;

// ── Explainer: implements LiftOps directly ─────────────────

/// Computation tracing. Implements LiftOps<N, R, N> for all Clone types.
pub struct Explainer;

impl<N: Clone + 'static, R: Clone + 'static> LiftOps<N, R, N> for Explainer {
    type LiftedH<H: Clone + 'static> = EH<N, H, R>;
    type LiftedR<H: Clone + 'static> = ER<N, H, R>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }

    fn lift_fold<H: Clone + 'static>(&self, original: Fold<N, H, R>) -> Fold<N, EH<N, H, R>, ER<N, H, R>> {
        let f1 = original.clone();
        let f2 = original.clone();
        let f3 = original;
        crate::domain::shared::fold::fold(
            move |node: &N| EH::new(node.clone(), f1.init(node)),
            move |heap: &mut EH<N, H, R>, result: &ER<N, H, R>| {
                f2.accumulate(&mut heap.working_heap, &result.orig_result);
                heap.transitions.push(ExplainerStep {
                    incoming_result: result.clone(),
                    resulting_heap: heap.working_heap.clone(),
                });
            },
            move |heap: &EH<N, H, R>| ER {
                orig_result: f3.finalize(&heap.working_heap),
                heap: heap.clone(),
            },
        )
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }

    fn unwrap<H: Clone + 'static>(&self, result: ER<N, H, R>) -> R {
        result.orig_result
    }
}

/// Treeish over ExplainerResult — each result's transitions are its children.
pub fn treeish_for_explres<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>(
) -> Treeish<ER<N, H, R>> {
    treeish(|x: &ER<N, H, R>| {
        x.heap.transitions.iter().map(|step| step.incoming_result.clone()).collect()
    })
}
