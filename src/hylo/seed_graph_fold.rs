use std::sync::Arc;
use either::Either;

use crate::fold::Fold;
use crate::ana::SeedGraph;
use crate::hylo::{GraphWithFold, HeapOfTopFn};

/// This struct builds on SeedGraph
/// - it formulates the RaCo using seed-centric heap construction
#[derive(Clone)]
pub struct SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT> {
    pub graph_spec: SeedGraph<NodeV, NodeE, Seed, Top>,
    pub(crate) impl_fold: Fold<Either<NodeE, NodeV>, Heap, ReturnT>,
    pub(crate) impl_top_to_heap: Arc<dyn Fn(&Top) -> Heap + Send + Sync>,
}

impl<NodeV, NodeE, Seed, Top, Heap, ReturnT> SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT>
where
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Heap: Clone + 'static,
    Seed: Clone + 'static,
    ReturnT: Clone + 'static,
{
    pub fn new(
        graph_spec: SeedGraph<NodeV, NodeE, Seed, Top>,
        fold_impl: Fold<Either<NodeE, NodeV>, Heap, ReturnT>,
        top_to_heap: impl Fn(&Top) -> Heap + Send + Sync + 'static,
    ) -> Self {
        SeedGraphFold {
            graph_spec,
            impl_fold: fold_impl,
            impl_top_to_heap: Arc::from(Box::new(top_to_heap) as HeapOfTopFn<Top, Heap>),
        }
    }

    pub fn top_to_heap(&self, top: &Top) -> Heap {
        (self.impl_top_to_heap)(top)
    }

    pub fn make_graph_with_fold(
        &self,
    ) -> GraphWithFold<Either<NodeE, NodeV>, Top, Heap, ReturnT> {
        let graph = self.graph_spec.make_graph();
        let run = self.impl_fold.clone();
        let top_to_heap = self.impl_top_to_heap.clone();
        GraphWithFold::new(
            &graph,
            &run,
            move |top| top_to_heap(top),
        )
    }

    pub fn execute(&self, strategy: crate::cata::Strategy, top: &Top) -> ReturnT
    where
        Either<NodeE, NodeV>: Send + Sync,
        Heap: Send + Sync,
        ReturnT: Send + Sync,
    {
        self.make_graph_with_fold().run(strategy, top)
    }

    pub fn map_top_to_heap<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(HeapOfTopFn<Top, Heap>) -> HeapOfTopFn<Top, Heap>,
    {
        let original_fn = self.impl_top_to_heap.clone();
        let boxed_original = Box::new(move |top: &Top| (*original_fn)(top));
        SeedGraphFold {
            graph_spec: self.graph_spec.clone(),
            impl_fold: self.impl_fold.clone(),
            impl_top_to_heap: Arc::from(mapper(boxed_original)),
        }
    }

    pub fn map_graph_spec<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(SeedGraph<NodeV, NodeE, Seed, Top>) -> SeedGraph<NodeV, NodeE, Seed, Top>,
    {
        SeedGraphFold {
            graph_spec: mapper(self.graph_spec.clone()),
            impl_fold: self.impl_fold.clone(),
            impl_top_to_heap: self.impl_top_to_heap.clone(),
        }
    }

    pub fn map_fold<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Fold<Either<NodeE, NodeV>, Heap, ReturnT>) -> Fold<Either<NodeE, NodeV>, Heap, ReturnT>,
    {
        SeedGraphFold {
            graph_spec: self.graph_spec.clone(),
            impl_fold: mapper(self.impl_fold.clone()),
            impl_top_to_heap: self.impl_top_to_heap.clone(),
        }
    }
}
