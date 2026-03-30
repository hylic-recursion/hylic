use std::sync::Arc;
use crate::graph::types::Treeish;
use crate::fold::Fold;
use crate::uio::UIO;

pub fn run<N, H, R>(fold: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> R
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    let ctx = Arc::new(Ctx {
            init: fold.impl_init.clone(),
            accumulate: fold.impl_accumulate.clone(),
            finalize: fold.impl_finalize.clone(),
            graph: graph.clone(),
    });
    build(&ctx, node).eval().clone()
}

struct Ctx<N, H, R> {
    init: Arc<dyn Fn(&N) -> H + Send + Sync>,
    accumulate: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    finalize: Arc<dyn Fn(&H) -> R + Send + Sync>,
    graph: Treeish<N>,
}

fn build<N, H, R>(ctx: &Arc<Ctx<N, H, R>>, node: &N) -> UIO<R>
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    let ctx = ctx.clone();
    let node = node.clone();
    UIO::new(move || {
        let mut child_uios = Vec::new();
        ctx.graph.visit(&node, &mut |child: &N| {
            child_uios.push(build(&ctx, child));
        });

        let mut heap = (ctx.init)(&node);
        if child_uios.len() <= 1 {
            for uio in &child_uios {
                (ctx.accumulate)(&mut heap, uio.eval());
            }
        } else {
            let results = UIO::join_par(child_uios);
            for r in results.eval() {
                (ctx.accumulate)(&mut heap, r);
            }
        }
        (ctx.finalize)(&heap)
    })
}
