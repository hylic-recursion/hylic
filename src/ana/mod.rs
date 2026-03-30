use std::sync::Arc;
use either::Either;

use crate::graph::types::{Edgy, Treeish, edgy_visit, treeish_visit};
use crate::graph::Graph;

pub(crate) type GrowNodeFn<Seed, NodeE, NodeV> = Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>;

pub struct SeedGraph<NodeV, NodeE, Seed, Top> {
    /// NodeV ->> Seed
    pub(crate) impl_seeds_from_valid_edgy: Edgy<NodeV, Seed>,
    /// Seed -> NodeV|NodeE
    pub(crate) impl_grow_node_fn: Arc<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>,
    /// Top ->> Seed
    pub(crate) impl_seeds_from_top: Edgy<Top, Seed>,
}

impl<NodeV, NodeE, Seed, Top> Clone for SeedGraph<NodeV, NodeE, Seed, Top> {
    fn clone(&self) -> Self {
        SeedGraph {
            impl_seeds_from_valid_edgy: self.impl_seeds_from_valid_edgy.clone(),
            impl_grow_node_fn: self.impl_grow_node_fn.clone(),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }
}

impl<NodeV, NodeE, Seed, Top> SeedGraph<NodeV, NodeE, Seed, Top>
where
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: 'static,
    Seed: Clone + 'static,
{
    pub fn new(
        seeds_from_valid_edgy: Edgy<NodeV, Seed>,
        grow_node_fn: impl Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync + 'static,
        seeds_from_top: Edgy<Top, Seed>,
    ) -> Self {
        SeedGraph {
            impl_seeds_from_valid_edgy: seeds_from_valid_edgy,
            impl_grow_node_fn: Arc::from(Box::new(grow_node_fn) as GrowNodeFn<Seed, NodeE, NodeV>),
            impl_seeds_from_top: seeds_from_top,
        }
    }

    pub fn seeds_from_valid(&self, node: &NodeV) -> Vec<Seed> {
        self.impl_seeds_from_valid_edgy.apply(node)
    }

    pub fn grow_node(&self, seed: &Seed) -> Either<NodeE, NodeV> {
        (self.impl_grow_node_fn)(seed)
    }

    pub fn seeds_from_top(&self, top: &Top) -> Vec<Seed> {
        self.impl_seeds_from_top.apply(top)
    }

    /// Build the recursive tree traversal. Valid nodes produce children
    /// via seeds; error nodes are leaves (no children).
    pub fn make_treeish(&self) -> Treeish<Either<NodeE, NodeV>> {
        let seeds = self.impl_seeds_from_valid_edgy.clone();
        let grow = self.impl_grow_node_fn.clone();
        treeish_visit(move |node: &Either<NodeE, NodeV>, cb: &mut dyn FnMut(&Either<NodeE, NodeV>)| {
            if let Either::Right(valid) = node {
                seeds.visit(valid, &mut |seed: &Seed| cb(&grow(seed)));
            }
        })
    }

    /// Build the top-level entry edgy: top → seeds → grown nodes.
    pub fn make_top_edgy(&self) -> Edgy<Top, Either<NodeE, NodeV>> {
        let seeds_fn = self.impl_seeds_from_top.clone();
        let grow = self.impl_grow_node_fn.clone();
        edgy_visit(move |top: &Top, cb: &mut dyn FnMut(&Either<NodeE, NodeV>)| {
            seeds_fn.visit(top, &mut |seed: &Seed| cb(&grow(seed)));
        })
    }

    /// Build the complete Graph from treeish + top_edgy.
    pub fn make_graph(&self) -> Graph<Top, Either<NodeE, NodeV>> {
        Graph::new(self.make_treeish(), self.make_top_edgy())
    }

    pub fn map_grow_node_fn<F>(&self, mapper: F) -> Self
    where F: FnOnce(GrowNodeFn<Seed, NodeE, NodeV>) -> GrowNodeFn<Seed, NodeE, NodeV> + 'static,
    {
        let orig = self.impl_grow_node_fn.clone();
        SeedGraph {
            impl_seeds_from_valid_edgy: self.impl_seeds_from_valid_edgy.clone(),
            impl_grow_node_fn: Arc::from(mapper(Box::new(move |seed: &Seed| (*orig)(seed)))),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }

    pub fn map_seeds_from_valid<F>(&self, mapper: F) -> Self
    where F: FnOnce(Edgy<NodeV, Seed>) -> Edgy<NodeV, Seed> + 'static,
    {
        SeedGraph {
            impl_seeds_from_valid_edgy: mapper(self.impl_seeds_from_valid_edgy.clone()),
            impl_grow_node_fn: self.impl_grow_node_fn.clone(),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }

    pub fn map_seeds_from_top<F>(&self, mapper: F) -> Self
    where F: FnOnce(Edgy<Top, Seed>) -> Edgy<Top, Seed> + 'static,
    {
        SeedGraph {
            impl_seeds_from_valid_edgy: self.impl_seeds_from_valid_edgy.clone(),
            impl_grow_node_fn: self.impl_grow_node_fn.clone(),
            impl_seeds_from_top: mapper(self.impl_seeds_from_top.clone()),
        }
    }
}
