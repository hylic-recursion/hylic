use std::sync::Arc;

use crate::fold::Fold;
use crate::utils::MapFn;
use crate::graph::graph::Graph;
use crate::hylo::GraphWithFold;

#[derive(Clone)]
pub struct FoldAdapter<NodeT, Top, HeapT, ReturnT> {
    pub graph_with_raco: GraphWithFold<NodeT, Top, HeapT, ReturnT>,
}

impl<NodeT, Top, HeapT, ReturnT> FoldAdapter<NodeT, Top, HeapT, ReturnT>
where
    NodeT: Clone + 'static,
    Top: Clone + 'static,
    HeapT: Clone + 'static,
    ReturnT: Clone + 'static,
{
    pub fn new(
        graph_with_raco: &GraphWithFold<NodeT, Top, HeapT, ReturnT>,
    ) -> Self {
        FoldAdapter {
            graph_with_raco: graph_with_raco.clone(),
        }
    }

    pub fn new_from_parts(
        graph: &Graph<Top, NodeT>,
        fold_impl: &Fold<NodeT, HeapT, ReturnT>,
        heap_of_top_fn: impl Fn(&Top) -> HeapT + Send + Sync + 'static,
    ) -> Self {
        FoldAdapter {
            graph_with_raco: GraphWithFold::new(
                graph,
                fold_impl,
                heap_of_top_fn,
            ),
        }
    }
    
    pub fn heap_of_top(&self, top: &Top) -> HeapT {
        self.graph_with_raco.heap_of_top(top)
    }

    pub fn run_node(&self, node: &NodeT) -> ReturnT {
        use crate::cata::sync;
        sync::run(
            &self.graph_with_raco.fold_impl,
            &self.graph_with_raco.graph.treeish,
            node,
        )
    }

    pub fn run_top(&self, top: &Top) -> ReturnT {
        self.graph_with_raco.run(top)
    }
    
    pub fn map_heap_of_top<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<Box<dyn Fn(&Top) -> HeapT + Send + Sync>> + 'static,
    {
        let original_fn = self.graph_with_raco.impl_heap_of_top.clone();
        let boxed_original = Box::new(move |top: &Top| (*original_fn)(top));
        
        Self {
            graph_with_raco: GraphWithFold {
                graph: self.graph_with_raco.graph.clone(),
                fold_impl: self.graph_with_raco.fold_impl.clone(),
                impl_heap_of_top: Arc::from(mapper(boxed_original)),
            },
        }
    }
    
    pub fn map_graph<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<Graph<Top, NodeT>> + 'static,
    {
        Self {
            graph_with_raco: GraphWithFold {
                graph: mapper(self.graph_with_raco.graph.clone()),
                fold_impl: self.graph_with_raco.fold_impl.clone(),
                impl_heap_of_top: self.graph_with_raco.impl_heap_of_top.clone(),
            },
        }
    }
    
    pub fn map_fold<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<Fold<NodeT, HeapT, ReturnT>> + 'static,
    {
        Self {
            graph_with_raco: GraphWithFold {
                graph: self.graph_with_raco.graph.clone(),
                fold_impl: mapper(self.graph_with_raco.fold_impl.clone()),
                impl_heap_of_top: self.graph_with_raco.impl_heap_of_top.clone(),
            },
        }
    }
    
    /// Maps the return type of this adapter to a new type using mapper and backmapper functions
    pub fn map<ReturnNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> FoldAdapter<NodeT, Top, HeapT, ReturnNew>
    where
        ReturnNew: Clone + 'static,
        MapF: Fn(&ReturnT) -> ReturnNew + Send + Sync + 'static,
        BackF: Fn(&ReturnNew) -> ReturnT + Send + Sync + 'static,
    {
        let gwr = self.graph_with_raco.clone();
        FoldAdapter::new_from_parts(
            &gwr.graph,
            &gwr.fold_impl.map(mapper, backmapper),
            move |top| (gwr.impl_heap_of_top)(top),
        )
    }
    
    /// Maps the return type to a tuple of (original, zipped) using the mapper function
    pub fn zipmap<ReturnZip, MapF>(&self, mapper: MapF) -> FoldAdapter<NodeT, Top, HeapT, (ReturnT, ReturnZip)>
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
