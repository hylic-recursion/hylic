use std::sync::Arc;
use crate::graph::Graph;
use crate::fold::Fold;
use crate::cata::Exec;
use super::HeapOfTopFn;

pub struct GraphWithFold<NodeT, Top, HeapT, ReturnT> {
    pub graph: Graph<Top, NodeT>,
    pub(crate) impl_heap_of_top: Arc<dyn Fn(&Top) -> HeapT + Send + Sync>,
    pub fold_impl: Fold<NodeT, HeapT, ReturnT>,
}

impl<NodeT, Top, HeapT, ReturnT> Clone for GraphWithFold<NodeT, Top, HeapT, ReturnT> {
    fn clone(&self) -> Self {
        GraphWithFold {
            graph: self.graph.clone(),
            impl_heap_of_top: self.impl_heap_of_top.clone(),
            fold_impl: self.fold_impl.clone(),
        }
    }
}

impl<NodeT, Top, HeapT, ReturnT> GraphWithFold<NodeT, Top, HeapT, ReturnT>
where
    NodeT: 'static,
    Top: 'static,
    HeapT: 'static,
    ReturnT: 'static,
{
    pub fn new(
        graph: &Graph<Top, NodeT>,
        fold_impl: &Fold<NodeT, HeapT, ReturnT>,
        heap_of_top_fn: impl Fn(&Top) -> HeapT + Send + Sync + 'static,
    ) -> Self {
        GraphWithFold {
            graph: graph.clone(),
            impl_heap_of_top: Arc::from(Box::new(heap_of_top_fn) as HeapOfTopFn<Top, HeapT>),
            fold_impl: fold_impl.clone(),
        }
    }

    pub fn heap_of_top(&self, top: &Top) -> HeapT {
        (self.impl_heap_of_top)(top)
    }

    pub fn run_node(&self, exec: &Exec<NodeT, ReturnT>, node: &NodeT) -> ReturnT {
        exec.run(&self.fold_impl, &self.graph.treeish, node)
    }

    pub fn run(&self, exec: &Exec<NodeT, ReturnT>, top: &Top) -> ReturnT {
        let mut heap = (self.impl_heap_of_top)(top);
        self.graph.top_edgy.visit(top, &mut |child| {
            let result = exec.run(&self.fold_impl, &self.graph.treeish, child);
            self.fold_impl.accumulate(&mut heap, &result);
        });
        self.fold_impl.finalize(&heap)
    }

    pub fn map_heap_of_top<F>(&self, mapper: F) -> Self
    where F: FnOnce(HeapOfTopFn<Top, HeapT>) -> HeapOfTopFn<Top, HeapT> + 'static,
    {
        let orig = self.impl_heap_of_top.clone();
        GraphWithFold {
            graph: self.graph.clone(),
            fold_impl: self.fold_impl.clone(),
            impl_heap_of_top: Arc::from(mapper(Box::new(move |top: &Top| (*orig)(top)))),
        }
    }

    pub fn map_graph<F>(&self, mapper: F) -> Self
    where F: FnOnce(Graph<Top, NodeT>) -> Graph<Top, NodeT> + 'static,
    {
        GraphWithFold {
            graph: mapper(self.graph.clone()),
            fold_impl: self.fold_impl.clone(),
            impl_heap_of_top: self.impl_heap_of_top.clone(),
        }
    }

    pub fn map_fold<F>(&self, mapper: F) -> Self
    where F: FnOnce(Fold<NodeT, HeapT, ReturnT>) -> Fold<NodeT, HeapT, ReturnT> + 'static,
    {
        GraphWithFold {
            graph: self.graph.clone(),
            fold_impl: mapper(self.fold_impl.clone()),
            impl_heap_of_top: self.impl_heap_of_top.clone(),
        }
    }

    pub fn map<ReturnNew: 'static, MapF, BackF>(
        &self, mapper: MapF, backmapper: BackF,
    ) -> GraphWithFold<NodeT, Top, HeapT, ReturnNew>
    where
        MapF: Fn(&ReturnT) -> ReturnNew + Send + Sync + 'static,
        BackF: Fn(&ReturnNew) -> ReturnT + Send + Sync + 'static,
    {
        let h = self.impl_heap_of_top.clone();
        GraphWithFold::new(
            &self.graph,
            &self.fold_impl.map(mapper, backmapper),
            move |top| h(top),
        )
    }

    pub fn zipmap<ReturnZip: 'static, MapF>(
        &self, mapper: MapF,
    ) -> GraphWithFold<NodeT, Top, HeapT, (ReturnT, ReturnZip)>
    where
        ReturnT: Clone,
        MapF: Fn(&ReturnT) -> ReturnZip + Send + Sync + 'static,
    {
        self.map(
            move |x| (x.clone(), mapper(x)),
            |x: &(ReturnT, ReturnZip)| x.0.clone(),
        )
    }
}

/// Convenience for Either-typed node graphs (seed-based resolution pattern).
impl<NodeE, NodeV, Top, HeapT, ReturnT> GraphWithFold<either::Either<NodeE, NodeV>, Top, HeapT, ReturnT>
where
    NodeE: 'static, NodeV: Clone + 'static, Top: 'static, HeapT: 'static, ReturnT: 'static,
{
    pub fn run_valid(&self, exec: &Exec<either::Either<NodeE, NodeV>, ReturnT>, node: &NodeV) -> ReturnT {
        self.run_node(exec, &either::Either::Right(node.clone()))
    }
}
