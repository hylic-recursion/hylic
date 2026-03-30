use std::sync::Arc;
use either::Either;


use crate::graph::types::{treeish_visit, Edgy, Treeish};
use crate::ana::ContramapFunc;

impl<NodeV, NodeE> Clone for TreeishFromErrEdgy<NodeV, NodeE> {
    fn clone(&self) -> Self {
        TreeishFromErrEdgy { impl_contramap_or: self.impl_contramap_or.clone(), impl_edges: self.impl_edges.clone() }
    }
}

pub struct TreeishFromErrEdgy<NodeV, NodeE> {
    pub(crate) impl_contramap_or: Arc<dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync>,
    pub(crate) impl_edges: Edgy<NodeV, Either<NodeE, NodeV>>,
}

impl<NodeV, NodeE> TreeishFromErrEdgy<NodeV, NodeE>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static
{
    pub fn contramap_or(&self, result: &Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> {
        (self.impl_contramap_or)(result)
    }
    
    pub fn edges(&self, node: &NodeV) -> Vec<Either<NodeE, NodeV>> {
        self.impl_edges.apply(node)
    }
    
    pub fn make_treeish(&self) -> Treeish<Either<NodeE, NodeV>> {
        let valid_edgy = self.impl_edges.clone();
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
            impl_edges: e,
        }
    }

    // Creates a TreeishFromErrEdgy with a custom contramap function
    pub fn new(
        e: Edgy<NodeV, Either<NodeE, NodeV>>,
        contramap_or: impl Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync + 'static
    ) -> Self {
        TreeishFromErrEdgy {
            impl_contramap_or: Arc::from(Box::new(contramap_or) as Box<ContramapFunc<NodeV, NodeE>>),
            impl_edges: e,
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
        F: FnOnce(Box<ContramapFunc<NodeV, NodeE>>) -> Box<ContramapFunc<NodeV, NodeE>> + 'static,
    {
        let original_fn = self.impl_contramap_or.clone();
        let boxed_original = Box::new(move |result: &Either<NodeE, NodeV>| (*original_fn)(result));
        TreeishFromErrEdgy {
            impl_contramap_or: Arc::from(mapper(boxed_original)),
            impl_edges: self.impl_edges.clone(),
        }
    }

    pub fn map_edges<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(Edgy<NodeV, Either<NodeE, NodeV>>) -> Edgy<NodeV, Either<NodeE, NodeV>> + 'static,
    {
        TreeishFromErrEdgy {
            impl_contramap_or: self.impl_contramap_or.clone(),
            impl_edges: mapper(self.impl_edges.clone()),
        }
    }
    
}
