//! Explainer: computation tracing as a Lift.
//!
//! Wraps a fold to record the full computation trace at every node —
//! initial heap, each child result accumulated, and the final result.
//! This is a histomorphism: each node sees its subtree's full history.
//!
//! Usage:
//!   Fused.run_lifted(&Explainer::lift(), ...)                 — trace discarded, get R
//!   Fused.run_lifted(&Explainer::lift_with(cb), ...)          — callback receives trace, get R
//!   Fused.run_lifted_zipped(&Explainer::lift(), ...)          — get (R, ExplainerResult)
//!   dom::DynExec::fused().run_lifted(&Explainer::lift(), ...) — same via runtime-dispatch wrapper

use crate::graph::{treeish, Treeish};
use crate::fold::Fold;
use crate::domain::Shared;
use crate::cata::Lift;

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

// ── Explainer ──────────────────────────────────────────────

/// Computation tracing as a Lift.
pub struct Explainer;

impl Explainer {
    /// Lift that records traces. Unwrap extracts the original R.
    pub fn lift<N, H, R>() -> Lift<Shared, N, H, R, N, EH<N, H, R>, ER<N, H, R>>
    where
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + 'static,
    {
        Lift::new(
            |treeish| treeish,
            Self::wrap_fold,
            |n: &N| n.clone(),
            |er: ER<N, H, R>| er.orig_result,
        )
    }

    /// Lift with callback: receives the full ExplainerResult before
    /// unwrapping to R. Use this to inspect or store the trace.
    pub fn lift_with<N, H, R>(
        on_result: impl Fn(&ER<N, H, R>) + Send + Sync + 'static,
    ) -> Lift<Shared, N, H, R, N, EH<N, H, R>, ER<N, H, R>>
    where
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + 'static,
    {
        Lift::new(
            |treeish| treeish,
            Self::wrap_fold,
            |n: &N| n.clone(),
            move |er: ER<N, H, R>| {
                on_result(&er);
                er.orig_result
            },
        )
    }

    fn wrap_fold<N, H, R>(original: Fold<N, H, R>) -> Fold<N, EH<N, H, R>, ER<N, H, R>>
    where
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + 'static,
    {
        let f1 = original.clone();
        let f2 = original.clone();
        let f3 = original;
        crate::fold::fold(
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
}

/// Treeish over ExplainerResult — each result's transitions are its children.
pub fn treeish_for_explres<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>(
) -> Treeish<ER<N, H, R>> {
    treeish(|x: &ER<N, H, R>| {
        x.heap.transitions.iter().map(|step| step.incoming_result.clone()).collect()
    })
}
