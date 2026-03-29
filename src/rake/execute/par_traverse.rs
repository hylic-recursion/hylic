use crate::graph::types::Treeish;
use crate::rake::RakeCompress;

pub fn recurse<N, H, R>(raco: &RakeCompress<N, H, R>, graph: &Treeish<N>, node: &N) -> R
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    let children = graph.apply(node);
    let results: Vec<R> = if children.len() <= 1 {
        children.iter().map(|c| recurse(raco, graph, c)).collect()
    } else {
        use rayon::prelude::*;
        children.par_iter().map(|c| recurse(raco, graph, c)).collect()
    };

    let mut heap = raco.rake_null(node);
    for r in &results { raco.rake_add(&mut heap, r); }
    raco.compress(&heap)
}
