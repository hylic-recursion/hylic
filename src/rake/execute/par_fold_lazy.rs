use std::sync::Arc;
use crate::graph::types::Treeish;
use crate::rake::RakeCompress;
use crate::rake::par::UIO;

pub fn run<N, H, R>(raco: &RakeCompress<N, H, R>, graph: &Treeish<N>, node: &N) -> R
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    let ctx = Arc::new(Ctx {
            rake_null: raco.impl_rake_null.clone(),
            rake_add: raco.impl_rake_add.clone(),
            compress: raco.impl_compress.clone(),
            graph: graph.clone(),
    });
    build(&ctx, node).eval().clone()
}

struct Ctx<N, H, R> {
    rake_null: Arc<dyn Fn(&N) -> H + Send + Sync>,
    rake_add: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    compress: Arc<dyn Fn(&H) -> R + Send + Sync>,
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

        let mut heap = (ctx.rake_null)(&node);
        if child_uios.len() <= 1 {
            for uio in &child_uios {
                (ctx.rake_add)(&mut heap, uio.eval());
            }
        } else {
            let results = UIO::join_par(child_uios);
            for r in results.eval() {
                (ctx.rake_add)(&mut heap, r);
            }
        }
        (ctx.compress)(&heap)
    })
}
