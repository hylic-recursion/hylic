use std::sync::Arc;
use super::types::{Edgy, Treeish};
use crate::rake::RakeCompress;


#[derive(Clone)]
pub struct Graph<Top, Node> {
    pub treeish: Treeish<Node>,
    pub top_edgy: Edgy<Top, Node>,
}

impl<Top, Node> Graph<Top, Node> {
    pub fn new(
        treeish: Treeish<Node>,
        top_edgy: Edgy<Top, Node>
    ) -> Self {
        Graph {
            treeish,
            top_edgy
        }
    }
}

#[derive(Clone)]
pub struct GraphWithRaco<NodeT, Top, HeapT, ReturnT> {
    pub graph: Graph<Top, NodeT>,
    pub impl_heap_of_top: Arc<dyn Fn(&Top) -> HeapT + Send + Sync>,
    pub rake_compress_impl: RakeCompress<NodeT, HeapT, ReturnT>,
}

impl<NodeT, Top, HeapT, ReturnT> GraphWithRaco<NodeT, Top, HeapT, ReturnT>
where
    NodeT: Clone + 'static,
    Top: Clone + 'static,
    HeapT: Clone + 'static,
    ReturnT: Clone + 'static,
{
    pub fn new(
        graph: &Graph<Top, NodeT>,
        rake_compress_impl: &RakeCompress<NodeT, HeapT, ReturnT>,
        heap_of_top_fn: impl Fn(&Top) -> HeapT + Send + Sync + 'static,
    ) -> Self {
        GraphWithRaco {
            graph: graph.clone(),
            impl_heap_of_top: Arc::from(Box::new(heap_of_top_fn) as Box<dyn Fn(&Top) -> HeapT + Send + Sync>),
            rake_compress_impl: rake_compress_impl.clone(),
        }
    }
    
    pub fn heap_of_top(&self, top: &Top) -> HeapT {
        (self.impl_heap_of_top)(top)
    }

    pub fn rake_compress(&self, top: &Top) -> ReturnT {
        use crate::rake::execute::sync;
        let mut heap = (self.impl_heap_of_top)(top);
        self.graph.top_edgy.visit(top, &mut |child| {
            let result = sync::recurse(&self.rake_compress_impl, &self.graph.treeish, child);
            self.rake_compress_impl.rake_add(&mut heap, &result);
        });
        self.rake_compress_impl.compress(&heap)
    }
}
