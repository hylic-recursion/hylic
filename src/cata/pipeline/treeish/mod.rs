//! TreeishPipeline — the honest-base pipeline for users who have
//! a `Treeish<N>` directly (no `grow: Seed → N` step). Two base
//! slots: `treeish` and `fold`. `Self::Seed = ()` — no Seed
//! dispatch at the executor boundary; use `run_from_node`.

use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;

pub mod reshape;
pub mod transforms;
pub mod source_impl;

pub struct TreeishPipeline<N, H, R> {
    pub(crate) treeish: Treeish<N>,
    pub(crate) fold:    Fold<N, H, R>,
}

impl<N, H, R> Clone for TreeishPipeline<N, H, R> {
    fn clone(&self) -> Self {
        TreeishPipeline {
            treeish: self.treeish.clone(),
            fold:    self.fold.clone(),
        }
    }
}

impl<N, H, R> TreeishPipeline<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    pub fn new(treeish: Treeish<N>, fold: &Fold<N, H, R>) -> Self {
        TreeishPipeline { treeish, fold: fold.clone() }
    }
}
