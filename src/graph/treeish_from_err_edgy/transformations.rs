use std::sync::Arc;
use either::Either;
use crate::graph::types::Edgy;
use crate::graph::treeish_from_err_edgy::TreeishFromErrEdgy;
use crate::utils::MapFn;

use super::super::ContramapFunc;

type EdgyT<NodeV, EdgeT> = Edgy<NodeV, EdgeT>;

pub fn map_contramap_or<NodeV, NodeE, F>(
    treeish_from_err_edgy: &TreeishFromErrEdgy<NodeV, NodeE>,
    mapper: F
) -> TreeishFromErrEdgy<NodeV, NodeE>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    F: MapFn<Box<ContramapFunc<NodeV, NodeE>>> + 'static,
{
    let original_fn = treeish_from_err_edgy.impl_contramap_or.clone();
    let boxed_original = Box::new(move |result: &Either<NodeE, NodeV>| (*original_fn)(result));
    
    TreeishFromErrEdgy {
        impl_contramap_or: Arc::from(mapper(boxed_original)),
        impl_edgy_valid: treeish_from_err_edgy.impl_edgy_valid.clone(),
    }
}

pub fn map_edgy_valid<NodeV, NodeE, F>(
    treeish_from_err_edgy: &TreeishFromErrEdgy<NodeV, NodeE>,
    mapper: F
) -> TreeishFromErrEdgy<NodeV, NodeE>
where 
    NodeV: Clone + 'static,
    NodeE: Clone + 'static,
    F: MapFn<EdgyT<NodeV, Either<NodeE, NodeV>>> + 'static,
{
    TreeishFromErrEdgy {
        impl_contramap_or: treeish_from_err_edgy.impl_contramap_or.clone(),
        impl_edgy_valid: mapper(treeish_from_err_edgy.impl_edgy_valid.clone()),
    }
}

