use crate::graph::{treeish, Treeish};

use super::{
    RakeCompress,
    rake_compress,
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
pub struct ExplainerHeapStep<N, H, R> where
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
    pub transitions: Vec<ExplainerHeapStep<N, H, R>>,

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
pub struct ExplainerRacoImpl<N, H, R> where
N: Clone, H: Clone, R: Clone,
{
    pub orig_rake_compress: RakeCompress<N, H, R>,
}

impl<N, H, R> ExplainerRacoImpl<N, H, R> where
N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    pub fn new(rake_compress: RakeCompress<N, H, R>) -> Self {
        ExplainerRacoImpl {
            orig_rake_compress: rake_compress,
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

impl<N, H, R> ExplainerRacoImpl<N, H, R> where
N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{

    pub fn of(rake_compress: RakeCompress<N, H, R>) -> Self {
        ExplainerRacoImpl::new(rake_compress)
    }

    pub fn wrap(&self) -> RakeCompress<EN<N>, EH<N, H, R>, ER<N, H, R>> {
        let impl_rake_null = self.orig_rake_compress.impl_rake_null.clone();
        let impl_rake_add = self.orig_rake_compress.impl_rake_add.clone();
        let impl_compress = self.orig_rake_compress.impl_compress.clone();
        rake_compress::<EN<N>, EH<N,H,R>, ER<N,H,R>>(
            move |node: &EN<N>| {
                EH::new(node.node.clone(), impl_rake_null(&node.node))
            },
            move |heap: &mut EH<N, H, R>, result: &ER<N, H, R>| {
                {
                    let heap_orig_mut: &mut H = &mut heap._wip_heap;
                    let result_orig: &R = &result.orig_result;
                    impl_rake_add(heap_orig_mut, result_orig);
                }
                heap.transitions.push(ExplainerHeapStep {
                    incoming_result: result.clone(),
                    resulting_heap: heap._wip_heap.clone(),
                });
            },
            move |heap: &EH<N, H, R>| {
                let orig_result = impl_compress(&heap._wip_heap);
                ER {
                    orig_result,
                    heap: heap.clone(),
                }
            },
        )
    }

    pub fn explain(&self, graph: &Treeish<N>, node: &N) -> ExplainerResult<N,H,R>
    where N: Clone,
    {
        use super::execute::sync;
        let wrapped_raco = self.wrap();
        let wrapped_treeish = graph.treemap(
            move |node: &N| EN::new(node.clone()),
            move |node: &EN<N>| node.node.clone(),
        );
        sync::recurse(&wrapped_raco, &wrapped_treeish, &EN::new(node.clone()))
    }

    pub fn execute_raked<HEx, REx>(&self,
        graph: &Treeish<N>,
        raco_explainer: &RakeCompress<ER<N,H,R>, HEx, REx>,
        node: &N,
    ) -> (R, REx)
    where N: Clone,
    {
        use super::execute::sync;
        let wrapped_result = self.explain(graph, node);
        let treeish_for_result: Treeish<ER<N,H,R>> = treeish_for_explres();
        let raked = sync::recurse(raco_explainer, &treeish_for_result, &wrapped_result);
        (wrapped_result.orig_result, raked)
    }

}


