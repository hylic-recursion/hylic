use std::sync::Arc;
use crate::graph::Graph;
use crate::fold::Fold;

#[derive(Clone)]
pub struct GraphWithFold<NodeT, Top, HeapT, ReturnT> {
    pub graph: Graph<Top, NodeT>,
    pub impl_heap_of_top: Arc<dyn Fn(&Top) -> HeapT + Send + Sync>,
    pub fold_impl: Fold<NodeT, HeapT, ReturnT>,
}

impl<NodeT, Top, HeapT, ReturnT> GraphWithFold<NodeT, Top, HeapT, ReturnT>
where
    NodeT: Clone + 'static,
    Top: Clone + 'static,
    HeapT: Clone + 'static,
    ReturnT: Clone + 'static,
{
    pub fn new(
        graph: &Graph<Top, NodeT>,
        fold_impl: &Fold<NodeT, HeapT, ReturnT>,
        heap_of_top_fn: impl Fn(&Top) -> HeapT + Send + Sync + 'static,
    ) -> Self {
        GraphWithFold {
            graph: graph.clone(),
            impl_heap_of_top: Arc::from(Box::new(heap_of_top_fn) as Box<dyn Fn(&Top) -> HeapT + Send + Sync>),
            fold_impl: fold_impl.clone(),
        }
    }

    pub fn heap_of_top(&self, top: &Top) -> HeapT {
        (self.impl_heap_of_top)(top)
    }

    pub fn run(&self, top: &Top) -> ReturnT {
        use crate::cata::sync;
        let mut heap = (self.impl_heap_of_top)(top);
        self.graph.top_edgy.visit(top, &mut |child| {
            let result = sync::run(&self.fold_impl, &self.graph.treeish, child);
            self.fold_impl.accumulate(&mut heap, &result);
        });
        self.fold_impl.finalize(&heap)
    }
}
