use crate::graph::types::Treeish;
use crate::fold::Fold;

pub fn run<N, H, R>(raco: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> R
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    let children = graph.apply(node);
    let results: Vec<R> = if children.len() <= 1 {
        children.iter().map(|c| run(raco, graph, c)).collect()
    } else {
        use rayon::prelude::*;
        children.par_iter().map(|c| run(raco, graph, c)).collect()
    };

    let mut heap = raco.init(node);
    for r in &results { raco.accumulate(&mut heap, r); }
    raco.finalize(&heap)
}
