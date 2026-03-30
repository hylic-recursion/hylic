use crate::graph::{treeish, Treeish};
use crate::fold::Fold;

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
    pub fn new(node: N, heap: H) -> Self {
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

#[derive(Clone)]
pub struct Explainer<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub orig_fold: Fold<N, H, R>,
}

impl<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static> Explainer<N, H, R> {
    pub fn new(fold: Fold<N, H, R>) -> Self {
        Explainer { orig_fold: fold }
    }

    /// Wrap the original fold to record computation traces.
    /// The wrapped fold operates on N directly — no type change needed.
    pub fn wrap(&self) -> Fold<N, EH<N, H, R>, ER<N, H, R>> {
        let f1 = self.orig_fold.clone();
        let f2 = self.orig_fold.clone();
        let f3 = self.orig_fold.clone();
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

    /// Run the wrapped fold on a graph, returning the full trace.
    pub fn explain(
        &self,
        exec: &crate::cata::Exec<N, ER<N, H, R>>,
        graph: &Treeish<N>,
        node: &N,
    ) -> ER<N, H, R> {
        exec.run(&self.wrap(), graph, node)
    }

    /// Run the wrapped fold, then fold the trace with a second fold.
    pub fn explain_and_fold<HEx: 'static, REx: Clone + 'static>(
        &self,
        exec: &crate::cata::Exec<N, ER<N, H, R>>,
        exec_result: &crate::cata::Exec<ER<N, H, R>, REx>,
        graph: &Treeish<N>,
        fold_explainer: &Fold<ER<N, H, R>, HEx, REx>,
        node: &N,
    ) -> (R, REx) {
        let result = self.explain(exec, graph, node);
        let treeish = treeish_for_explres::<N, H, R>();
        let folded = exec_result.run(fold_explainer, &treeish, &result);
        (result.orig_result, folded)
    }
}

/// Treeish over ExplainerResult — each result's transitions are its children.
pub fn treeish_for_explres<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>(
) -> Treeish<ER<N, H, R>> {
    treeish(|x: &ER<N, H, R>| {
        x.heap.transitions.iter().map(|step| step.incoming_result.clone()).collect()
    })
}
