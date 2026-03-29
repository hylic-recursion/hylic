use crate::graph::treeish_from_deperr::TreeishFromDepErr;
use crate::graph::EdgyFromDepErr;
use crate::graph::OptContramapFuncRc;
use crate::utils::MapFn;


pub fn map_edgy_from_deperr<NodeV, NodeE, HeapSeed, F>(
    treeish_from_deperr: &TreeishFromDepErr<NodeV, NodeE, HeapSeed>,
    mapper: F
) -> TreeishFromDepErr<NodeV, NodeE, HeapSeed>
where
    NodeV: Clone + 'static, 
    NodeE: Clone + 'static,
    HeapSeed: Clone + 'static,
    F: MapFn<EdgyFromDepErr<NodeV, NodeE, HeapSeed>> + 'static,
{
    TreeishFromDepErr {
        impl_edgy_from_deperr: mapper(treeish_from_deperr.impl_edgy_from_deperr.clone()),
        impl_contramap_or: treeish_from_deperr.impl_contramap_or.clone(),
    }
}

pub fn map_contramap_or<NodeV, NodeE, HeapSeed, F>(
    treeish_from_deperr: &TreeishFromDepErr<NodeV, NodeE, HeapSeed>,
    mapper: F
) -> TreeishFromDepErr<NodeV, NodeE, HeapSeed>
where
    NodeV: Clone + 'static, 
    NodeE: Clone + 'static,
    HeapSeed: Clone + 'static,
    F: MapFn<OptContramapFuncRc<NodeV, NodeE>> + 'static,
{
    TreeishFromDepErr {
        impl_edgy_from_deperr: treeish_from_deperr.impl_edgy_from_deperr.clone(),
        impl_contramap_or: mapper(treeish_from_deperr.impl_contramap_or.clone()),
    }
}

