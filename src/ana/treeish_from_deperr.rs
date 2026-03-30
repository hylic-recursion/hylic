use std::sync::Arc;

use either::Either;
use super::edgy_from_deperr::EdgyFromDepErr;
use crate::ana::OptContramapFuncRc;

use crate::graph::types::Treeish;
use crate::ana::treeish_from_err_edgy::TreeishFromErrEdgy;


#[derive(Clone)]
pub struct TreeishFromDepErr<NodeV, NodeE, HeapSeed> {
    pub(crate) impl_edgy_from_deperr: EdgyFromDepErr<NodeV, NodeE, HeapSeed>,
    pub(crate) impl_contramap_or: Option<Arc<dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync>>,
}

impl <NodeV, NodeE, HeapSeed> TreeishFromDepErr<NodeV, NodeE, HeapSeed>
where
    NodeV: Clone + 'static, 
    NodeE: Clone + 'static,
    HeapSeed: Clone + 'static,
{
    // Creates a new TreeishFromDepErr with no contramap
    pub fn new(
        edgy_from_deperr: EdgyFromDepErr<NodeV, NodeE, HeapSeed>
    ) -> Self {
        TreeishFromDepErr {
            impl_edgy_from_deperr: edgy_from_deperr,
            impl_contramap_or: None,
        }
    }
    
    // Creates a new TreeishFromDepErr with a contramap function
    pub fn new_with_contramap(
        edgy_from_deperr: EdgyFromDepErr<NodeV, NodeE, HeapSeed>,
        contramap: impl Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync + 'static
    ) -> Self {
        let f_boxed: Box<dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync> = Box::new(contramap);
        
        TreeishFromDepErr {
            impl_edgy_from_deperr: edgy_from_deperr,
            impl_contramap_or: Some(Arc::from(f_boxed)),
        }
    }
    
    pub fn contramap_or(&self, result: &Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> {
        match &self.impl_contramap_or {
            Some(contramap) => contramap(result),
            None => match result {
                either::Right(valid) => Either::Right(valid.clone()),
                either::Left(_) => Either::Left(vec![])
            }
        }
    }

    pub fn make_treeish(&self) -> Treeish<Either<NodeE, NodeV>> {
        let edgy_from_deperr = self.impl_edgy_from_deperr.clone();
        let f_contramap_or = self.impl_contramap_or.clone();
        
        let treeish_from_err_edgy = match f_contramap_or {
            Some(contramap_or) => TreeishFromErrEdgy::new(
                edgy_from_deperr.make_edgy(),
                move |result| contramap_or(result)
            ),
            None => TreeishFromErrEdgy::new_default(
                edgy_from_deperr.make_edgy()
            )
        };
        
        treeish_from_err_edgy.make_treeish()
    }
    
    pub fn map_edgy_from_deperr<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(EdgyFromDepErr<NodeV, NodeE, HeapSeed>) -> EdgyFromDepErr<NodeV, NodeE, HeapSeed> + 'static,
    {
        TreeishFromDepErr {
            impl_edgy_from_deperr: mapper(self.impl_edgy_from_deperr.clone()),
            impl_contramap_or: self.impl_contramap_or.clone(),
        }
    }

    pub fn map_contramap_or<F>(&self, mapper: F) -> Self
    where
        F: FnOnce(OptContramapFuncRc<NodeV, NodeE>) -> OptContramapFuncRc<NodeV, NodeE> + 'static,
    {
        TreeishFromDepErr {
            impl_edgy_from_deperr: self.impl_edgy_from_deperr.clone(),
            impl_contramap_or: mapper(self.impl_contramap_or.clone()),
        }
    }
    
}
