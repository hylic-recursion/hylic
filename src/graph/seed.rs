use std::sync::Arc;

use crate::graph::types::{Edgy, Treeish, edgy_visit, treeish_visit};
use crate::graph::Graph;

/// General seed-based anamorphism. No assumption about Node type —
/// works with any Node, whether fallible (Either) or not.
///
/// Three functions define the unfolding:
/// - seeds_from_node: given a node, what are its dependency seeds?
/// - grow: given a seed, produce a node
/// - seeds_from_top: given a top-level entry point, what are the initial seeds?
pub struct SeedGraph<Node, Seed, Top> {
    pub(crate) impl_seeds_from_node: Edgy<Node, Seed>,
    pub(crate) impl_grow: Arc<dyn Fn(&Seed) -> Node + Send + Sync>,
    pub(crate) impl_seeds_from_top: Edgy<Top, Seed>,
}

impl<Node, Seed, Top> Clone for SeedGraph<Node, Seed, Top> {
    fn clone(&self) -> Self {
        SeedGraph {
            impl_seeds_from_node: self.impl_seeds_from_node.clone(),
            impl_grow: self.impl_grow.clone(),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }
}

impl<Node, Seed, Top> SeedGraph<Node, Seed, Top>
where Node: 'static, Seed: 'static, Top: 'static,
{
    pub fn new(
        seeds_from_node: Edgy<Node, Seed>,
        grow: impl Fn(&Seed) -> Node + Send + Sync + 'static,
        seeds_from_top: Edgy<Top, Seed>,
    ) -> Self {
        SeedGraph {
            impl_seeds_from_node: seeds_from_node,
            impl_grow: Arc::from(Box::new(grow) as Box<dyn Fn(&Seed) -> Node + Send + Sync>),
            impl_seeds_from_top: seeds_from_top,
        }
    }

    pub fn seeds_from_node(&self, node: &Node) -> Vec<Seed> where Seed: Clone {
        self.impl_seeds_from_node.apply(node)
    }

    pub fn grow(&self, seed: &Seed) -> Node {
        (self.impl_grow)(seed)
    }

    pub fn seeds_from_top(&self, top: &Top) -> Vec<Seed> where Seed: Clone {
        self.impl_seeds_from_top.apply(top)
    }

    /// Build the recursive tree traversal: node → seeds → grow each → children.
    pub fn make_treeish(&self) -> Treeish<Node> {
        let seeds = self.impl_seeds_from_node.clone();
        let grow = self.impl_grow.clone();
        treeish_visit(move |node: &Node, cb: &mut dyn FnMut(&Node)| {
            seeds.visit(node, &mut |seed: &Seed| cb(&grow(seed)));
        })
    }

    /// Build the top-level entry edgy: top → seeds → grow each.
    pub fn make_top_edgy(&self) -> Edgy<Top, Node> {
        let seeds = self.impl_seeds_from_top.clone();
        let grow = self.impl_grow.clone();
        edgy_visit(move |top: &Top, cb: &mut dyn FnMut(&Node)| {
            seeds.visit(top, &mut |seed: &Seed| cb(&grow(seed)));
        })
    }

    /// Build the complete Graph from treeish + top_edgy.
    pub fn make_graph(&self) -> Graph<Top, Node> {
        Graph::new(self.make_treeish(), self.make_top_edgy())
    }

    pub fn map_grow<F>(&self, mapper: F) -> Self
    where F: FnOnce(Box<dyn Fn(&Seed) -> Node + Send + Sync>) -> Box<dyn Fn(&Seed) -> Node + Send + Sync> + 'static,
    {
        let orig = self.impl_grow.clone();
        SeedGraph {
            impl_seeds_from_node: self.impl_seeds_from_node.clone(),
            impl_grow: Arc::from(mapper(Box::new(move |seed: &Seed| (*orig)(seed)))),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }

    pub fn map_seeds_from_node<F>(&self, mapper: F) -> Self
    where F: FnOnce(Edgy<Node, Seed>) -> Edgy<Node, Seed> + 'static,
    {
        SeedGraph {
            impl_seeds_from_node: mapper(self.impl_seeds_from_node.clone()),
            impl_grow: self.impl_grow.clone(),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }

    pub fn map_seeds_from_top<F>(&self, mapper: F) -> Self
    where F: FnOnce(Edgy<Top, Seed>) -> Edgy<Top, Seed> + 'static,
    {
        SeedGraph {
            impl_seeds_from_node: self.impl_seeds_from_node.clone(),
            impl_grow: self.impl_grow.clone(),
            impl_seeds_from_top: mapper(self.impl_seeds_from_top.clone()),
        }
    }
}
