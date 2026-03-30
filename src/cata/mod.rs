pub mod sync;
mod par_traverse;
mod par_fold_lazy;
pub mod par;

#[cfg(test)]
mod tests;

use crate::graph::types::Treeish;
use crate::fold::Fold;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Sequential,
    ParTraverse,
    ParFoldLazy,
}

pub use Strategy::*;

pub const ALL: [Strategy; 3] = [Sequential, ParTraverse, ParFoldLazy];

impl Strategy {
    pub fn run<N, H, R>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> R
    where
        N: Clone + Send + Sync + 'static,
        H: Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        match self {
            Sequential => sync::run(fold, graph, node),
            ParTraverse => par_traverse::run(fold, graph, node),
            ParFoldLazy => par_fold_lazy::run(fold, graph, node),
        }
    }
}
