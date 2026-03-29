use std::sync::Arc;
use either::Either;

use crate::graph::types::Edgy;
use crate::graph::graph_with_seed_and_err::{
    GraphWithSeedAndErr,
    GraphWithSeedAndErrRaco
};
use crate::rake::RakeCompress;
use crate::utils::MapFn;

type EdgyT<A, B> = Edgy<A, B>;
type FuncTopToHeap<Top, Heap> = Box<dyn Fn(&Top) -> Heap + Send + Sync>;

pub fn map_grow_node_fn<NodeV, NodeE, Seed, Top, F>(
    graph_spec: &GraphWithSeedAndErr<NodeV, NodeE, Seed, Top>,
    mapper: F
) -> GraphWithSeedAndErr<NodeV, NodeE, Seed, Top>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    F: MapFn<Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>> + 'static,
{
    let original_fn = graph_spec.impl_grow_node_fn.clone();
    let boxed_original = Box::new(move |seed: &Seed| (*original_fn)(seed));
    
    GraphWithSeedAndErr {
        impl_seeds_from_valid_edgy: graph_spec.impl_seeds_from_valid_edgy.clone(),
        impl_grow_node_fn: Arc::from(mapper(boxed_original)),
        impl_seeds_from_top: graph_spec.impl_seeds_from_top.clone(),
    }
}

pub fn map_seeds_from_valid<NodeV, NodeE, Seed, Top, F>(
    graph_spec: &GraphWithSeedAndErr<NodeV, NodeE, Seed, Top>,
    mapper: F
) -> GraphWithSeedAndErr<NodeV, NodeE, Seed, Top>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    F: MapFn<EdgyT<NodeV, Seed>> + 'static,
{
    GraphWithSeedAndErr {
        impl_seeds_from_valid_edgy: mapper(graph_spec.impl_seeds_from_valid_edgy.clone()),
        impl_grow_node_fn: graph_spec.impl_grow_node_fn.clone(),
        impl_seeds_from_top: graph_spec.impl_seeds_from_top.clone(),
    }
}

pub fn map_seeds_from_top<NodeV, NodeE, Seed, Top, F>(
    graph_spec: &GraphWithSeedAndErr<NodeV, NodeE, Seed, Top>,
    mapper: F
) -> GraphWithSeedAndErr<NodeV, NodeE, Seed, Top>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    F: MapFn<EdgyT<Top, Seed>> + 'static,
{
    GraphWithSeedAndErr {
        impl_seeds_from_valid_edgy: graph_spec.impl_seeds_from_valid_edgy.clone(),
        impl_grow_node_fn: graph_spec.impl_grow_node_fn.clone(),
        impl_seeds_from_top: mapper(graph_spec.impl_seeds_from_top.clone()),
    }
}


pub fn map_top_to_heap<NodeV, NodeE, Seed, Top, Heap, ReturnT, F>(
    raco: &GraphWithSeedAndErrRaco<NodeV, NodeE, Seed, Top, Heap, ReturnT>,
    mapper: F
) -> GraphWithSeedAndErrRaco<NodeV, NodeE, Seed, Top, Heap, ReturnT>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    Heap: Clone + 'static,
    ReturnT: Clone + 'static,
    F: MapFn<FuncTopToHeap<Top, Heap>>,
{
    let original_fn = raco.impl_top_to_heap.clone();
    let boxed_original = Box::new(move |top: &Top| (*original_fn)(top));
    
    GraphWithSeedAndErrRaco {
        graph_spec: raco.graph_spec.clone(),
        impl_rake_compress: raco.impl_rake_compress.clone(),
        impl_top_to_heap: Arc::from(mapper(boxed_original)),
    }
}

pub fn map_graph_spec<NodeV, NodeE, Seed, Top, Heap, ReturnT, F>(
    raco: &GraphWithSeedAndErrRaco<NodeV, NodeE, Seed, Top, Heap, ReturnT>,
    mapper: F
) -> GraphWithSeedAndErrRaco<NodeV, NodeE, Seed, Top, Heap, ReturnT>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    Heap: Clone + 'static,
    ReturnT: Clone + 'static,
    F: MapFn<GraphWithSeedAndErr<NodeV, NodeE, Seed, Top>>,
{
    GraphWithSeedAndErrRaco {
        graph_spec: mapper(raco.graph_spec.clone()),
        impl_rake_compress: raco.impl_rake_compress.clone(),
        impl_top_to_heap: raco.impl_top_to_heap.clone(),
    }
}

pub fn map_rake_compress<NodeV, NodeE, Seed, Top, Heap, ReturnT, F>(
    raco: &GraphWithSeedAndErrRaco<NodeV, NodeE, Seed, Top, Heap, ReturnT>,
    mapper: F
) -> GraphWithSeedAndErrRaco<NodeV, NodeE, Seed, Top, Heap, ReturnT>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    Heap: Clone + 'static,
    ReturnT: Clone + 'static,
    F: MapFn<RakeCompress<Either<NodeE, NodeV>, Heap, ReturnT>>,
{
    GraphWithSeedAndErrRaco {
        graph_spec: raco.graph_spec.clone(),
        impl_rake_compress: mapper(raco.impl_rake_compress.clone()),
        impl_top_to_heap: raco.impl_top_to_heap.clone(),
    }
}

