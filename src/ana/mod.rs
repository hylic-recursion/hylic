use std::sync::Arc;
use either::Either;

use crate::graph::types::{Edgy, Treeish, edgy_visit};
use crate::graph::Graph;

pub mod edgy_from_deperr;
pub mod treeish_from_deperr;
pub mod treeish_from_err_edgy;

pub type ContramapFunc<NodeV, NodeE> = dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync;

use edgy_from_deperr::EdgyFromDepErr;
use treeish_from_deperr::TreeishFromDepErr;

#[derive(Clone)]
pub struct SeedGraph<NodeV, NodeE, Seed, Top> {
    /// NodeV ->> Seed
    pub(crate) impl_seeds_from_valid_edgy: Edgy<NodeV, Seed>,

    /// Seed -> NodeV|NodeE
    pub(crate) impl_grow_node_fn: Arc<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>,

    /// Top ->> Seed
    pub(crate) impl_seeds_from_top: Edgy<Top, Seed>,
}

impl <NodeV, NodeE, Seed, Top> SeedGraph<NodeV, NodeE, Seed, Top> 
where
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    Top: Clone + 'static,
    Seed: Clone + 'static,
{
    pub fn new(
        seeds_from_valid_edgy: Edgy<NodeV, Seed>,
        grow_node_fn: impl Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync + 'static,
        seeds_from_top: Edgy<Top, Seed>,
    ) -> Self {
        SeedGraph {
            impl_seeds_from_valid_edgy: seeds_from_valid_edgy,
            impl_grow_node_fn: Arc::from(Box::new(grow_node_fn) as Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>),
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

    // Creates an EdgyFromDepErr from the basic functions
    pub fn spec_edgy_from_deperr(&self) -> EdgyFromDepErr<NodeV, NodeE, Seed> {
        let grow_node_fn = self.impl_grow_node_fn.clone();
        EdgyFromDepErr::new(
            self.impl_seeds_from_valid_edgy.clone(), 
            move |seed| (grow_node_fn)(seed)
        )
    }

    // Creates a TreeishFromDepErr from the edgy_spec
    pub fn spec_treeish_from_deperr(&self) -> TreeishFromDepErr<NodeV, NodeE, Seed> {
        TreeishFromDepErr::new(self.spec_edgy_from_deperr())
    }

    // Creates a Treeish from the TreeishFromDepErr
    pub fn make_treeish(&self) -> Treeish<Either<NodeE, NodeV>> {
        self.spec_treeish_from_deperr().make_treeish()
    }

    // Creates the edgy that yields the top "Node"s, an ingredient for the Graph
    pub fn make_top_edgy(&self) -> Edgy<Top, Either<NodeE, NodeV>> {
        let seeds_fn = self.impl_seeds_from_top.clone();
        let grow_node_fn = self.impl_grow_node_fn.clone();

        edgy_visit(move |node: &Top, cb: &mut dyn FnMut(&Either<NodeE, NodeV>)| {
            seeds_fn.visit(node, &mut |seed: &Seed| {
                let grown = (grow_node_fn)(seed);
                cb(&grown);
            });
        })
    }

    // Creates the Graph from treeish and top_edgy
    pub fn make_graph(&self) -> Graph<Top, Either<NodeE, NodeV>> {
        Graph::new(
            self.make_treeish(),
            self.make_top_edgy()
        )
    }

    pub fn map_grow_node_fn<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>) -> Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync> + 'static,
    {
        let original_fn = self.impl_grow_node_fn.clone();
        let boxed_original = Box::new(move |seed: &Seed| (*original_fn)(seed));
        SeedGraph {
            impl_seeds_from_valid_edgy: self.impl_seeds_from_valid_edgy.clone(),
            impl_grow_node_fn: Arc::from(mapper(boxed_original)),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }

    pub fn map_seeds_from_valid<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Edgy<NodeV, Seed>) -> Edgy<NodeV, Seed> + 'static,
    {
        SeedGraph {
            impl_seeds_from_valid_edgy: mapper(self.impl_seeds_from_valid_edgy.clone()),
            impl_grow_node_fn: self.impl_grow_node_fn.clone(),
            impl_seeds_from_top: self.impl_seeds_from_top.clone(),
        }
    }

    pub fn map_seeds_from_top<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Edgy<Top, Seed>) -> Edgy<Top, Seed> + 'static,
    {
        SeedGraph {
            impl_seeds_from_valid_edgy: self.impl_seeds_from_valid_edgy.clone(),
            impl_grow_node_fn: self.impl_grow_node_fn.clone(),
            impl_seeds_from_top: mapper(self.impl_seeds_from_top.clone()),
        }
    }
}
