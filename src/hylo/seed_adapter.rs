use either::Either;

use crate::fold::Fold;
use crate::ana::SeedGraph;
use crate::cata::Exec;
use crate::hylo::adapter::FoldAdapter;
use super::HeapOfTopFn;

/// SeedFoldAdapter wraps SeedGraph + FoldAdapter for seed-based resolution.
#[derive(Clone)]
pub struct SeedFoldAdapter<NodeV, NodeE, Seed, Top, HeapT, ReturnT> {
    pub graph_with_seed_and_err: SeedGraph<NodeV, NodeE, Seed, Top>,
    pub core_adapter: FoldAdapter<Either<NodeE, NodeV>, Top, HeapT, ReturnT>,
}

impl <NodeV, NodeE, Seed, Top, HeapT, ReturnT> SeedFoldAdapter<NodeV, NodeE, Seed, Top, HeapT, ReturnT>
where
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Seed: Clone + 'static,
    Top: Clone + 'static,
    HeapT: Clone + 'static,
    ReturnT: Clone + 'static,
{
    pub fn new(
        graph_with_seed_and_err: SeedGraph<NodeV, NodeE, Seed, Top>,
        fold_impl: Fold<Either<NodeE, NodeV>, HeapT, ReturnT>,
        heap_of_top_fn: impl Fn(&Top) -> HeapT + Send + Sync + 'static,
    ) -> Self {
        let graph = graph_with_seed_and_err.make_graph();
        let core_adapter = FoldAdapter::new_from_parts(&graph, &fold_impl, heap_of_top_fn);
        SeedFoldAdapter { graph_with_seed_and_err, core_adapter }
    }

    pub fn heap_of_top(&self, top: &Top) -> HeapT {
        self.core_adapter.heap_of_top(top)
    }

    pub fn run_node(&self, exec: &Exec<Either<NodeE, NodeV>, ReturnT>, node: &Either<NodeE, NodeV>) -> ReturnT {
        self.core_adapter.run_node(exec, node)
    }

    pub fn run_valid(&self, exec: &Exec<Either<NodeE, NodeV>, ReturnT>, node: &NodeV) -> ReturnT
    where NodeV: Clone, NodeE: Clone,
    {
        self.run_node(exec, &Either::Right(node.clone()))
    }

    pub fn run_top(&self, exec: &Exec<Either<NodeE, NodeV>, ReturnT>, top: &Top) -> ReturnT {
        self.core_adapter.run_top(exec, top)
    }

    pub fn map_graph_with_seed_and_err<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(SeedGraph<NodeV, NodeE, Seed, Top>) -> SeedGraph<NodeV, NodeE, Seed, Top> + 'static,
    {
        let new_graph_with_seed_and_err = mapper(self.graph_with_seed_and_err.clone());
        let new_graph = new_graph_with_seed_and_err.make_graph();
        Self {
            graph_with_seed_and_err: new_graph_with_seed_and_err,
            core_adapter: self.core_adapter.map_graph(move |_| new_graph.clone()),
        }
    }

    pub fn map_heap_of_top<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(HeapOfTopFn<Top, HeapT>) -> HeapOfTopFn<Top, HeapT> + 'static,
    {
        Self {
            graph_with_seed_and_err: self.graph_with_seed_and_err.clone(),
            core_adapter: self.core_adapter.map_heap_of_top(mapper),
        }
    }

    pub fn map<ReturnNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF)
        -> SeedFoldAdapter<NodeV, NodeE, Seed, Top, HeapT, ReturnNew>
    where
        ReturnNew: Clone + 'static,
        MapF: Fn(&ReturnT) -> ReturnNew + Send + Sync + 'static,
        BackF: Fn(&ReturnNew) -> ReturnT + Send + Sync + 'static,
    {
        SeedFoldAdapter {
            graph_with_seed_and_err: self.graph_with_seed_and_err.clone(),
            core_adapter: self.core_adapter.map(mapper, backmapper),
        }
    }

    pub fn zipmap<ReturnZip, MapF>(&self, mapper: MapF)
        -> SeedFoldAdapter<NodeV, NodeE, Seed, Top, HeapT, (ReturnT, ReturnZip)>
    where
        ReturnZip: Clone + 'static,
        MapF: Fn(&ReturnT) -> ReturnZip + Send + Sync + 'static,
    {
        SeedFoldAdapter {
            graph_with_seed_and_err: self.graph_with_seed_and_err.clone(),
            core_adapter: self.core_adapter.zipmap(mapper),
        }
    }
}
