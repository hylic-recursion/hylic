//! Rayon executor: collect children, par_iter for parallel recursion.
//!
//! Zero-sized — no Arc, no boxing. Fold and graph are borrowed by
//! shared reference through rayon's scoped parallelism. The Send+Sync
//! bounds are on this module's impl only.

use crate::fold::Fold;
use crate::graph::Treeish;
use super::super::Executor;

/// Parallel executor via rayon's work-stealing thread pool.
#[derive(Clone, Copy, Debug)]
pub struct Rayon;

impl<N, R> Executor<N, R> for Rayon
where
    N: Clone + Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        recurse(fold, graph, root)
    }
}

fn recurse<N, H: 'static, R>(
    fold: &Fold<N, H, R>,
    graph: &Treeish<N>,
    node: &N,
) -> R
where
    N: Clone + Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    use rayon::prelude::*;

    let mut heap = fold.init(node);
    let children = graph.apply(node);

    if children.len() <= 1 {
        for child in &children {
            fold.accumulate(&mut heap, &recurse(fold, graph, child));
        }
    } else {
        let results: Vec<R> = children.par_iter()
            .map(|c| recurse(fold, graph, c))
            .collect();
        for r in &results { fold.accumulate(&mut heap, r); }
    }

    fold.finalize(&heap)
}
