//! Explainer: computation tracing as a Lift.
//!
//! Wraps a fold to record the full computation trace at every node —
//! initial heap, each child result accumulated, and the final result.
//! This is a histomorphism: each node sees its subtree's full history.

use crate::graph::{treeish, Treeish};
use crate::fold::Fold;
use crate::cata::{Exec, Lift};

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

/// Computation tracing. Records the full fold execution trace.
pub struct Explainer;

impl Explainer {
    /// Lift: wraps a fold to record traces. Unwrap extracts the
    /// original result — the trace is computed but discarded.
    /// Use `explain()` to get the trace.
    pub fn lift<N, H, R>() -> Lift<N, H, R, N, EH<N, H, R>, ER<N, H, R>>
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

    /// Run the fold with tracing, returning the full ExplainerResult.
    pub fn explain<N, H, R>(
        exec: &Exec<N, ER<N, H, R>>,
        fold: &Fold<N, H, R>,
        graph: &Treeish<N>,
        root: &N,
    ) -> ER<N, H, R>
    where
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + 'static,
    {
        exec.run(&Self::wrap_fold(fold.clone()), graph, root)
    }

    /// Run traced, then fold the trace tree with a second fold.
    pub fn explain_and_fold<N, H, R, HEx: 'static, REx: Clone + 'static>(
        exec: &Exec<N, ER<N, H, R>>,
        exec_result: &Exec<ER<N, H, R>, REx>,
        fold: &Fold<N, H, R>,
        fold_explainer: &Fold<ER<N, H, R>, HEx, REx>,
        graph: &Treeish<N>,
        root: &N,
    ) -> (R, REx)
    where
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + 'static,
    {
        let result = Self::explain(exec, fold, graph, root);
        let treeish = treeish_for_explres::<N, H, R>();
        let folded = exec_result.run(fold_explainer, &treeish, &result);
        (result.orig_result, folded)
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
