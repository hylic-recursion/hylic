use std::sync::Arc;
use either::Either;


use crate::graph::types::{treeish_visit, Edgy, Treeish};
use crate::graph::ContramapFunc;
use crate::utils::MapFn;

pub mod transformations;

#[derive(Clone)]
pub struct TreeishFromErrEdgy<NodeV, NodeE> {
    pub(crate) impl_contramap_or: Arc<dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync>,
    pub(crate) impl_edgy_valid: Edgy<NodeV, Either<NodeE, NodeV>>,
}

impl<NodeV, NodeE> TreeishFromErrEdgy<NodeV, NodeE>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static
{
    pub fn contramap_or(&self, result: &Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> {
        (self.impl_contramap_or)(result)
    }
    
    pub fn edgy_valid(&self, node: &NodeV) -> Vec<Either<NodeE, NodeV>> {
        self.impl_edgy_valid.apply(node)
    }
    
    pub fn make_treeish(&self) -> Treeish<Either<NodeE, NodeV>> {
        let valid_edgy = self.impl_edgy_valid.clone();
        let contramap = self.impl_contramap_or.clone();

        treeish_visit(move |v: &Either<NodeE, NodeV>, cb: &mut dyn FnMut(&Either<NodeE, NodeV>)| {
            match v {
                either::Right(valid) => valid_edgy.visit(valid, cb),
                either::Left(_) => match (contramap)(v) {
                    Either::Right(node) => valid_edgy.visit(&node, cb),
                    Either::Left(edges) => { for e in &edges { cb(e); } }
                }
            }
        })
    }

    // Creates a TreeishFromErrEdgy with a default contramap
    pub fn new_default(e: Edgy<NodeV, Either<NodeE, NodeV>>) -> Self {
        TreeishFromErrEdgy {
            impl_contramap_or: {
                let f = Self::default_contramap();
                Arc::from(Box::new(move |v: &Either<NodeE, NodeV>| f(v)) as Box<ContramapFunc<NodeV, NodeE>>)
            },
            impl_edgy_valid: e,
        }
    }

    // Creates a TreeishFromErrEdgy with a custom contramap function
    pub fn new(
        e: Edgy<NodeV, Either<NodeE, NodeV>>,
        contramap_or: impl Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync + 'static
    ) -> Self {
        TreeishFromErrEdgy {
            impl_contramap_or: Arc::from(Box::new(contramap_or) as Box<ContramapFunc<NodeV, NodeE>>),
            impl_edgy_valid: e,
        }
    }

    pub fn default_contramap_err_case(_err: &NodeE) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> {
        Either::Left(vec![])
    }

    pub fn default_contramap() -> impl Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> {
        |v| match v {
            either::Right(valid) => Either::Right(valid.clone()),
            either::Left(err) => Self::default_contramap_err_case(err),
        }
    }
    
    pub fn map_contramap_or<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<Box<dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync>> + 'static,
    {
        transformations::map_contramap_or(self, mapper)
    }
    
    pub fn map_edgy_valid<F>(&self, mapper: F) -> Self
    where 
        F: MapFn<Edgy<NodeV, Either<NodeE, NodeV>>> + 'static,
    {
        transformations::map_edgy_valid(self, mapper)
    }
    
}
