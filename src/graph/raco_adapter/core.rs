use std::sync::Arc;

use crate::rake::RakeCompress;
use crate::utils::MapFn;
use crate::graph::graph::Graph;
use crate::graph::graph::GraphWithRaco;

#[derive(Clone)]
pub struct RacoAdapter<NodeT, Top, HeapT, ReturnT> {
    pub graph_with_raco: GraphWithRaco<NodeT, Top, HeapT, ReturnT>,
}

impl<NodeT, Top, HeapT, ReturnT> RacoAdapter<NodeT, Top, HeapT, ReturnT>
where
    NodeT: Clone + 'static,
    Top: Clone + 'static,
    HeapT: Clone + 'static,
    ReturnT: Clone + 'static,
{
    pub fn new(
        graph_with_raco: &GraphWithRaco<NodeT, Top, HeapT, ReturnT>,
    ) -> Self {
        RacoAdapter {
            graph_with_raco: graph_with_raco.clone(),
        }
    }

    pub fn new_from_parts(
        graph: &Graph<Top, NodeT>,
        rake_compress_impl: &RakeCompress<NodeT, HeapT, ReturnT>,
        heap_of_top_fn: impl Fn(&Top) -> HeapT + Send + Sync + 'static,
    ) -> Self {
        RacoAdapter {
            graph_with_raco: GraphWithRaco::new(
                graph,
                rake_compress_impl,
                heap_of_top_fn,
            ),
        }
    }
    
    pub fn heap_of_top(&self, top: &Top) -> HeapT {
        self.graph_with_raco.heap_of_top(top)
    }

    pub fn execute_on_node(&self, node: &NodeT) -> ReturnT {
        use crate::rake::execute::sync;
        sync::recurse(
            &self.graph_with_raco.rake_compress_impl,
            &self.graph_with_raco.graph.treeish,
            node,
        )
    }

    pub fn execute_top(&self, top: &Top) -> ReturnT {
        self.graph_with_raco.rake_compress(top)
    }
    
    pub fn map_heap_of_top<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<Box<dyn Fn(&Top) -> HeapT + Send + Sync>> + 'static,
    {
        let original_fn = self.graph_with_raco.impl_heap_of_top.clone();
        let boxed_original = Box::new(move |top: &Top| (*original_fn)(top));
        
        Self {
            graph_with_raco: GraphWithRaco {
                graph: self.graph_with_raco.graph.clone(),
                rake_compress_impl: self.graph_with_raco.rake_compress_impl.clone(),
                impl_heap_of_top: Arc::from(mapper(boxed_original)),
            },
        }
    }
    
    pub fn map_graph<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<Graph<Top, NodeT>> + 'static,
    {
        Self {
            graph_with_raco: GraphWithRaco {
                graph: mapper(self.graph_with_raco.graph.clone()),
                rake_compress_impl: self.graph_with_raco.rake_compress_impl.clone(),
                impl_heap_of_top: self.graph_with_raco.impl_heap_of_top.clone(),
            },
        }
    }
    
    pub fn map_rake_compress<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<RakeCompress<NodeT, HeapT, ReturnT>> + 'static,
    {
        Self {
            graph_with_raco: GraphWithRaco {
                graph: self.graph_with_raco.graph.clone(),
                rake_compress_impl: mapper(self.graph_with_raco.rake_compress_impl.clone()),
                impl_heap_of_top: self.graph_with_raco.impl_heap_of_top.clone(),
            },
        }
    }
    
    /// Maps the return type of this adapter to a new type using mapper and backmapper functions
    pub fn map<ReturnNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> RacoAdapter<NodeT, Top, HeapT, ReturnNew>
    where
        ReturnNew: Clone + 'static,
        MapF: Fn(&ReturnT) -> ReturnNew + Send + Sync + 'static,
        BackF: Fn(&ReturnNew) -> ReturnT + Send + Sync + 'static,
    {
        let gwr = self.graph_with_raco.clone();
        RacoAdapter::new_from_parts(
            &gwr.graph,
            &gwr.rake_compress_impl.map(mapper, backmapper),
            move |top| (gwr.impl_heap_of_top)(top),
        )
    }
    
    /// Maps the return type to a tuple of (original, zipped) using the mapper function
    pub fn zipmap<ReturnZip, MapF>(&self, mapper: MapF) -> RacoAdapter<NodeT, Top, HeapT, (ReturnT, ReturnZip)>
    where
        ReturnZip: Clone + 'static,
        MapF: Fn(&ReturnT) -> ReturnZip + Send + Sync + 'static,
    {
        let backmap = |x: &(ReturnT, ReturnZip)| x.0.clone();
        
        self.map(
            move |x| (x.clone(), mapper(x)),
            backmap,
        )
    }
}
