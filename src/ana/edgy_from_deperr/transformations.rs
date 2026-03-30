use std::sync::Arc;
use either::Either;

use crate::ana::edgy_from_deperr::EdgyFromDepErr;
use crate::utils::{MapFn, EdgyMapFn};


pub fn map_edgy_seed<NodeV, NodeE, Seed, F>(
    edgy_from_deperr: &EdgyFromDepErr<NodeV, NodeE, Seed>,
    mapper: F
) -> EdgyFromDepErr<NodeV, NodeE, Seed>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Seed: Clone + 'static,
    F: EdgyMapFn<NodeV, Seed>,
{
    EdgyFromDepErr {
        impl_edgy_seed: mapper(edgy_from_deperr.impl_edgy_seed.clone()),
        impl_grow_node: edgy_from_deperr.impl_grow_node.clone(),
    }
}

pub fn map_grow_node<NodeV, NodeE, Seed, F>(
    edgy_from_deperr: &EdgyFromDepErr<NodeV, NodeE, Seed>,
    mapper: F
) -> EdgyFromDepErr<NodeV, NodeE, Seed>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Seed: Clone + 'static,
    F: MapFn<Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>>,
{
    let original_fn = edgy_from_deperr.impl_grow_node.clone();
    let boxed_original = Box::new(move |seed: &Seed| (*original_fn)(seed));
    
    EdgyFromDepErr {
        impl_edgy_seed: edgy_from_deperr.impl_edgy_seed.clone(),
        impl_grow_node: Arc::from(mapper(boxed_original)),
    }
}

