use std::sync::Arc;
use crate::graph::types::Treeish;
use crate::fold::Fold;
use crate::uio::UIO;

pub type PlanTransformFn<R> = Box<dyn Fn(UIO<R>) -> UIO<R> + Send + Sync>;

/// Composed parallel executor. Builds a UIO computation plan from
/// a fold + graph, applies an optional plan transformation, evaluates
/// with rayon parallelism via join_par.
///
/// `Strategy::Par` delegates to `Par::new(fold).run(graph, root)`.
pub struct Par<N, H, R>
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    fold: Fold<N, H, R>,
    plan_transform: Arc<PlanTransformFn<R>>,
}

impl<N, H, R> Par<N, H, R>
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    pub fn new(fold: &Fold<N, H, R>) -> Self {
        Par {
            fold: fold.clone(),
            plan_transform: Arc::new(Box::new(|plan| plan)),
        }
    }

    pub fn map_fold<F>(&self, mapper: F) -> Self
    where F: FnOnce(Fold<N, H, R>) -> Fold<N, H, R>,
    {
        Par {
            fold: mapper(self.fold.clone()),
            plan_transform: self.plan_transform.clone(),
        }
    }

    pub fn map_plan_transform<F>(&self, mapper: F) -> Self
    where F: FnOnce(PlanTransformFn<R>) -> PlanTransformFn<R> + 'static,
    {
        let orig = self.plan_transform.clone();
        Par {
            fold: self.fold.clone(),
            plan_transform: Arc::new(mapper(Box::new(move |plan| orig(plan)))),
        }
    }

    /// Build the UIO computation plan without evaluating.
    /// The returned `UIO<R>` is a tree of deferred computations
    /// linked by `join_par`. Calling `.eval()` triggers parallel execution.
    pub fn build(&self, graph: &Treeish<N>, root: &N) -> UIO<R> {
        let plan = build_uio(&self.fold, graph, root);
        (self.plan_transform)(plan)
    }

    /// Build + evaluate: parallel fold execution.
    pub fn run(&self, graph: &Treeish<N>, root: &N) -> R {
        self.build(graph, root).eval().clone()
    }
}

// The UIO tree construction — graph discovery fused inside UIO closures.
// Each UIO, when evaluated: discover children, build their UIOs,
// evaluate children in parallel (join_par), accumulate, finalize.
fn build_uio<N, H, R>(fold: &Fold<N, H, R>, graph: &Treeish<N>, node: &N) -> UIO<R>
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
    build_inner(&ctx, node)
}

struct Ctx<N, H, R> {
    init: Arc<dyn Fn(&N) -> H + Send + Sync>,
    accumulate: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    finalize: Arc<dyn Fn(&H) -> R + Send + Sync>,
    graph: Treeish<N>,
}

fn build_inner<N, H, R>(ctx: &Arc<Ctx<N, H, R>>, node: &N) -> UIO<R>
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
            child_uios.push(build_inner(&ctx, child));
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
