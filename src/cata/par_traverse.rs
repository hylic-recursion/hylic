use crate::graph::types::Treeish;
use crate::fold::Fold;

pub(crate) fn run<N, H, R>(fold: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> R
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    let children = graph.apply(node);
    let results: Vec<R> = if children.len() <= 1 {
        children.iter().map(|c| run(fold, graph, c)).collect()
    } else {
        use rayon::prelude::*;
        children.par_iter().map(|c| run(fold, graph, c)).collect()
    };

    let mut heap = fold.init(node);
    for r in &results { fold.accumulate(&mut heap, r); }
    fold.finalize(&heap)
}
