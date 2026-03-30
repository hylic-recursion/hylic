use std::sync::Arc;
use either::Either;

use crate::fold::Fold;
use crate::graph::types::{Edgy, Treeish, edgy_visit};
use crate::graph::Graph;
use crate::hylo::GraphWithFold;

pub mod transformations;
pub mod edgy_from_deperr;
pub mod treeish_from_deperr;
pub mod treeish_from_err_edgy;

pub type ContramapFunc<NodeV, NodeE> = dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync;
pub type OptContramapFunc<NodeV, NodeE> = Option<Box<ContramapFunc<NodeV, NodeE>>>;
pub type OptContramapFuncRc<NodeV, NodeE> = Option<Arc<ContramapFunc<NodeV, NodeE>>>;

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

    pub fn to_fold<Heap, ResultT>(
        &self,
        fold_impl: Fold<Either<NodeE, NodeV>, Heap, ResultT>,
        top_to_heap: impl Fn(&Top) -> Heap + Send + Sync + 'static,
    ) -> SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ResultT> where
        // TODO: reduce this
        NodeV: Clone + 'static,
        NodeE: Clone + 'static,
        Top: Clone + 'static,
        Seed: Clone + 'static,
        Heap: Clone + 'static,
        ResultT: Clone + 'static,
        {
        SeedGraphFold::new(
            self.clone(),
            fold_impl,
            top_to_heap,
        )
    }
    
    pub fn map_grow_node_fn<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync>) -> Box<dyn Fn(&Seed) -> Either<NodeE, NodeV> + Send + Sync> + 'static,
    {
        transformations::map_grow_node_fn(self, mapper)
    }

    pub fn map_seeds_from_valid<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Edgy<NodeV, Seed>) -> Edgy<NodeV, Seed> + 'static,
    {
        transformations::map_seeds_from_valid(self, mapper)
    }

    pub fn map_seeds_from_top<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Edgy<Top, Seed>) -> Edgy<Top, Seed> + 'static,
    {
        transformations::map_seeds_from_top(self, mapper)
    }
}


/// this struct builds on SeedGraph
/// - it formulates the RaCo using seed-centric heap construction
#[derive(Clone)]
pub struct SeedGraphFold<NodeV, NodeE, Seed, Top, Heap, ReturnT> {
    pub graph_spec: SeedGraph<NodeV, NodeE, Seed, Top>,
    pub(crate) impl_fold: Fold<Either<NodeE, NodeV>, Heap, ReturnT>,
    pub(crate) impl_top_to_heap: Arc<dyn Fn(&Top) -> Heap + Send + Sync>,
    // pub seed_to_heap: Arc<dyn Fn(&Seed) -> Heap>>,
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
            impl_top_to_heap: Arc::from(Box::new(top_to_heap) as Box<dyn Fn(&Top) -> Heap + Send + Sync>),
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

    pub fn execute(&self, top: &Top) -> ReturnT {
        self.make_graph_with_fold().run(top)
    }
    
    pub fn map_top_to_heap<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Box<dyn Fn(&Top) -> Heap + Send + Sync>) -> Box<dyn Fn(&Top) -> Heap + Send + Sync>,
    {
        transformations::map_top_to_heap(self, mapper)
    }

    pub fn map_graph_spec<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(SeedGraph<NodeV, NodeE, Seed, Top>) -> SeedGraph<NodeV, NodeE, Seed, Top>,
    {
        transformations::map_graph_spec(self, mapper)
    }

    pub fn map_fold<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(crate::fold::Fold<Either<NodeE, NodeV>, Heap, ReturnT>) -> crate::fold::Fold<Either<NodeE, NodeV>, Heap, ReturnT>,
    {
        transformations::map_fold(self, mapper)
    }

}


