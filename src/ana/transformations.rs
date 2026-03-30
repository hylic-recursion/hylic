use std::sync::Arc;
use either::Either;

use crate::graph::types::Edgy;
use crate::ana::{
    SeedGraph,
    SeedGraphFold
};
use crate::fold::Fold;
use crate::utils::MapFn;

type EdgyT<A, B> = Edgy<A, B>;
type FuncTopToHeap<Top, Heap> = Box<dyn Fn(&Top) -> Heap + Send + Sync>;

pub fn map_grow_node_fn<NodeV, NodeE, Seed, Top, F>(
    graph_spec: &SeedGraph<NodeV, NodeE, Seed, Top>,
    mapper: F
) -> SeedGraph<NodeV, NodeE, Seed, Top>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    F: MapFn<Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>> + 'static,
{
    let original_fn = graph_spec.impl_grow_node_fn.clone();
    let boxed_original = Box::new(move |seed: &Seed| (*original_fn)(seed));
    
    SeedGraph {
        impl_seeds_from_valid_edgy: graph_spec.impl_seeds_from_valid_edgy.clone(),
        impl_grow_node_fn: Arc::from(mapper(boxed_original)),
        impl_seeds_from_top: graph_spec.impl_seeds_from_top.clone(),
    }
}

pub fn map_seeds_from_valid<NodeV, NodeE, Seed, Top, F>(
    graph_spec: &SeedGraph<NodeV, NodeE, Seed, Top>,
    mapper: F
) -> SeedGraph<NodeV, NodeE, Seed, Top>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    F: MapFn<EdgyT<NodeV, Seed>> + 'static,
{
    SeedGraph {
        impl_seeds_from_valid_edgy: mapper(graph_spec.impl_seeds_from_valid_edgy.clone()),
        impl_grow_node_fn: graph_spec.impl_grow_node_fn.clone(),
        impl_seeds_from_top: graph_spec.impl_seeds_from_top.clone(),
    }
}

pub fn map_seeds_from_top<NodeV, NodeE, Seed, Top, F>(
    graph_spec: &SeedGraph<NodeV, NodeE, Seed, Top>,
    mapper: F
) -> SeedGraph<NodeV, NodeE, Seed, Top>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    F: MapFn<EdgyT<Top, Seed>> + 'static,
{
    SeedGraph {
        impl_seeds_from_valid_edgy: graph_spec.impl_seeds_from_valid_edgy.clone(),
        impl_grow_node_fn: graph_spec.impl_grow_node_fn.clone(),
        impl_seeds_from_top: mapper(graph_spec.impl_seeds_from_top.clone()),
    }
}


pub fn map_top_to_heap<NodeV, NodeE, Seed, Top, Heap, ReturnT, F>(
    raco: &SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT>,
    mapper: F
) -> SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT>
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
    
    SeedGraphFold {
        graph_spec: raco.graph_spec.clone(),
        impl_fold: raco.impl_fold.clone(),
        impl_top_to_heap: Arc::from(mapper(boxed_original)),
    }
}

pub fn map_graph_spec<NodeV, NodeE, Seed, Top, Heap, ReturnT, F>(
    raco: &SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT>,
    mapper: F
) -> SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    Heap: Clone + 'static,
    ReturnT: Clone + 'static,
    F: MapFn<SeedGraph<NodeV, NodeE, Seed, Top>>,
{
    SeedGraphFold {
        graph_spec: mapper(raco.graph_spec.clone()),
        impl_fold: raco.impl_fold.clone(),
        impl_top_to_heap: raco.impl_top_to_heap.clone(),
    }
}

pub fn map_fold<NodeV, NodeE, Seed, Top, Heap, ReturnT, F>(
    raco: &SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT>,
    mapper: F
) -> SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
    Heap: Clone + 'static,
    ReturnT: Clone + 'static,
    F: MapFn<Fold<Either<NodeE, NodeV>, Heap, ReturnT>>,
{
    SeedGraphFold {
        graph_spec: raco.graph_spec.clone(),
        impl_fold: mapper(raco.impl_fold.clone()),
        impl_top_to_heap: raco.impl_top_to_heap.clone(),
    }
}

