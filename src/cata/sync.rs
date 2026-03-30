use crate::graph::types::Treeish;
use crate::fold::Fold;

pub fn run<N: 'static, H: 'static, R: 'static>(fold: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> R {
    let mut heap = fold.init(node);
    graph.visit(node, &mut |child| {
        let result = run(fold, graph, child);
        fold.accumulate(&mut heap, &result);
    });
    fold.finalize(&heap)
}
