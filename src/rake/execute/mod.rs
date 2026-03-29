pub mod sync;
mod par_traverse;
mod par_fold_lazy;

use crate::graph::types::Treeish;
use crate::rake::RakeCompress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Executor {
    Sequential,
    ParTraverse,
    ParFoldLazy,
}

pub use Executor::*;

pub const ALL: [Executor; 3] = [Sequential, ParTraverse, ParFoldLazy];

impl Executor {
    pub fn run<N, H, R>(&self, raco: &RakeCompress<N, H, R>, graph: &Treeish<N>, node: &N) -> R
    where
        N: Clone + Send + Sync + 'static,
        H: Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        match self {
            Sequential => sync::recurse(raco, graph, node),
            ParTraverse => par_traverse::recurse(raco, graph, node),
            ParFoldLazy => par_fold_lazy::run(raco, graph, node),
        }
    }
}
