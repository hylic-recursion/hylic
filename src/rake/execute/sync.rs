use crate::graph::types::Treeish;
use crate::rake::RakeCompress;

pub fn recurse<N: 'static, H, R>(raco: &RakeCompress<N, H, R>, graph: &Treeish<N>, node: &N) -> R {
    let mut heap = raco.rake_null(node);
    graph.visit(node, &mut |child| {
        let result = recurse(raco, graph, child);
        raco.rake_add(&mut heap, &result);
    });
    raco.compress(&heap)
}
