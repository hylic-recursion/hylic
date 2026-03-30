use std::sync::Arc;

use either::Either;

use crate::graph::types::{Edgy, edgy_visit};
use super::GrowNodeFn;


impl<NodeV, NodeE, Seed> Clone for EdgyFromDepErr<NodeV, NodeE, Seed> {
    fn clone(&self) -> Self {
        EdgyFromDepErr { impl_seeds: self.impl_seeds.clone(), impl_grow_node: self.impl_grow_node.clone() }
    }
}

pub struct EdgyFromDepErr<NodeV, NodeE, Seed> {
    pub(crate) impl_seeds: Edgy<NodeV, Seed>,
    pub(crate) impl_grow_node: Arc<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>,
}

#[allow(dead_code)]
impl<NodeV, NodeE, Seed> EdgyFromDepErr<NodeV, NodeE, Seed> 
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Seed: Clone + 'static,
{
    pub fn new(
        seeds: Edgy<NodeV, Seed>,
        grow_node: impl Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync + 'static
    ) -> Self {
        EdgyFromDepErr {
            impl_seeds: seeds,
            impl_grow_node: Arc::from(Box::new(grow_node) as GrowNodeFn<Seed, NodeE, NodeV>),
        }
    }
    
    pub fn seeds(&self, node: &NodeV) -> Vec<Seed> {
        self.impl_seeds.apply(node)
    }
    
    pub fn grow_node(&self, seed: &Seed) -> Either<NodeE, NodeV> {
        (self.impl_grow_node)(seed)
    }
    
    pub fn make_edgy(&self) -> Edgy<NodeV, Either<NodeE, NodeV>> {
        let grow_node_fn = self.impl_grow_node.clone();
        let seeds = self.impl_seeds.clone();
        edgy_visit(move |node: &NodeV, cb: &mut dyn FnMut(&Either<NodeE, NodeV>)| {
            seeds.visit(node, &mut |seed: &Seed| {
                let grown = (grow_node_fn)(seed);
                cb(&grown);
            });
        })
    }
    
    pub fn map_seeds<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Edgy<NodeV, Seed>) -> Edgy<NodeV, Seed> + 'static,
    {
        EdgyFromDepErr {
            impl_seeds: mapper(self.impl_seeds.clone()),
            impl_grow_node: self.impl_grow_node.clone(),
        }
    }

    pub fn map_grow_node<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(GrowNodeFn<Seed, NodeE, NodeV>) -> GrowNodeFn<Seed, NodeE, NodeV>,
    {
        let original_fn = self.impl_grow_node.clone();
        let boxed_original = Box::new(move |seed: &Seed| (*original_fn)(seed));
        EdgyFromDepErr {
            impl_seeds: self.impl_seeds.clone(),
            impl_grow_node: Arc::from(mapper(boxed_original)),
        }
    }
    
}


