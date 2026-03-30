pub mod exec;

#[cfg(test)]
mod tests;

use crate::graph::types::Treeish;
use crate::fold::Fold;

pub use exec::{Exec, ChildVisitorFn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Sequential,
    Par,
}

pub use Strategy::*;

pub const ALL: [Strategy; 2] = [Sequential, Par];

impl Strategy {
    pub fn run<N, H, R>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> R
    where
        N: Clone + Send + Sync + 'static,
        H: Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        match self {
            Sequential => Exec::fused().run(fold, graph, node),
            Par => Exec::rayon().run(fold, graph, node),
        }
    }
}
