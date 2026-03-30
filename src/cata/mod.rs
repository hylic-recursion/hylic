pub mod sync;
mod par_traverse;
mod par_fold_lazy;

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
    pub fn run<N, H, R>(&self, raco: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> R
    where
        N: Clone + Send + Sync + 'static,
        H: Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        match self {
            Sequential => sync::run(raco, graph, node),
            ParTraverse => par_traverse::run(raco, graph, node),
            ParFoldLazy => par_fold_lazy::run(raco, graph, node),
        }
    }
}
