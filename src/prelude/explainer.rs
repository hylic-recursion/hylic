use crate::graph::{treeish, Treeish};

use crate::fold::{
    Fold,
};

#[derive(Clone)]
pub struct ExplainerNode<N> where
N: Clone,
{
    pub node: N,
}
impl <N> ExplainerNode<N> where N: Clone {
    pub fn new(node: N) -> Self {
        ExplainerNode { node }
    }
}

#[derive(Clone)]
pub struct ExplainerStep<N, H, R> where
N: Clone, H: Clone, R: Clone,
{
    pub incoming_result: ExplainerResult<N, H, R>,
    pub resulting_heap: H,
}

#[derive(Clone)]
pub struct ExplainerHeap<N, H, R> where
N: Clone, H: Clone, R: Clone,
{
    pub initial_heap: H,
    pub orig_node: ExplainerNode<N>,
    pub transitions: Vec<ExplainerStep<N, H, R>>,

    pub _wip_heap: H,
    // pub _phantom_R: std::marker::PhantomData<R>,
}
impl<N, H, R> ExplainerHeap<N, H, R> where
N: Clone, H: Clone, R: Clone,
{
    pub fn new(node: N, heap: H) -> Self {
        ExplainerHeap {
            initial_heap: heap.clone(),
            orig_node: ExplainerNode { node },
            transitions: Vec::new(),
            _wip_heap: heap,
        }
    }
}

#[derive(Clone)]
pub struct ExplainerResult<N, H, R> where
R: Clone, N: Clone, H: Clone,
{
    pub orig_result: R,
    pub heap: ExplainerHeap<N, H, R>,
}

type EN<N> = ExplainerNode<N>;
type EH<N, H, R> = ExplainerHeap<N, H, R>;
type ER<N, H, R> = ExplainerResult<N, H, R>;

#[derive(Clone)]
pub struct Explainer<N, H, R> where
N: Clone, H: Clone, R: Clone,
{
    pub orig_fold: Fold<N, H, R>,
}

impl<N, H, R> Explainer<N, H, R> where
N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    pub fn new(run: Fold<N, H, R>) -> Self {
        Explainer {
            orig_fold: run,
        }
    }
}

pub fn treeish_for_explres<N, H, R>() -> Treeish<ER<N, H, R>> where
N: Clone + 'static,
H: Clone + 'static,
R: Clone + 'static,
{
    treeish(|x: &ER<N,H,R>| {
        x.heap.transitions.iter().map(|step| {
            step.incoming_result.clone()
        }).collect::<Vec<_>>()
    })
}

impl<N, H, R> Explainer<N, H, R> where
N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{


    pub fn wrap(&self) -> Fold<EN<N>, EH<N, H, R>, ER<N, H, R>> {
        let impl_init = self.orig_fold.impl_init.clone();
        let impl_accumulate = self.orig_fold.impl_accumulate.clone();
        let impl_finalize = self.orig_fold.impl_finalize.clone();
        crate::fold::fold::<EN<N>, EH<N,H,R>, ER<N,H,R>>(
            move |node: &EN<N>| {
                EH::new(node.node.clone(), impl_init(&node.node))
            },
            move |heap: &mut EH<N, H, R>, result: &ER<N, H, R>| {
                {
                    let heap_orig_mut: &mut H = &mut heap._wip_heap;
                    let result_orig: &R = &result.orig_result;
                    impl_accumulate(heap_orig_mut, result_orig);
                }
                heap.transitions.push(ExplainerStep {
                    incoming_result: result.clone(),
                    resulting_heap: heap._wip_heap.clone(),
                });
            },
            move |heap: &EH<N, H, R>| {
                let orig_result = impl_finalize(&heap._wip_heap);
                ER {
                    orig_result,
                    heap: heap.clone(),
                }
            },
        )
    }

    pub fn explain(&self,
        exec: &crate::cata::Exec<EN<N>, ER<N, H, R>>,
        graph: &Treeish<N>,
        node: &N,
    ) -> ExplainerResult<N,H,R> {
        let wrapped_fold = self.wrap();
        let wrapped_treeish = graph.treemap(
            move |node: &N| EN::new(node.clone()),
            move |node: &EN<N>| node.node.clone(),
        );
        exec.run(&wrapped_fold, &wrapped_treeish, &EN::new(node.clone()))
    }

    pub fn explain_and_fold<HEx: 'static, REx: Clone + 'static>(&self,
        exec: &crate::cata::Exec<EN<N>, ER<N, H, R>>,
        exec_result: &crate::cata::Exec<ER<N, H, R>, REx>,
        graph: &Treeish<N>,
        fold_explainer: &Fold<ER<N,H,R>, HEx, REx>,
        node: &N,
    ) -> (R, REx) {
        let wrapped_result = self.explain(exec, graph, node);
        let treeish_for_result: Treeish<ER<N,H,R>> = treeish_for_explres();
        let folded = exec_result.run(fold_explainer, &treeish_for_result, &wrapped_result);
        (wrapped_result.orig_result, folded)
    }

}


