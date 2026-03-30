use std::sync::Arc;
use crate::graph::types::Treeish;
use crate::fold::Fold;

/// How children are visited and their results delivered.
/// The lambda encapsulates traversal mode (callback vs collect)
/// and parallelism (sequential vs rayon). Bounds like Send/Sync
/// are checked at construction, not here.
pub type ChildVisitorFn<N, R> = dyn Fn(
    &Treeish<N>,
    &N,
    &(dyn Fn(&N) -> R + Send + Sync),
    &mut dyn FnMut(&R),
) + Send + Sync;

/// Unified executor parameterized by a child-visiting lambda.
/// Fused (callback), unfused (collect), parallel (rayon) are
/// different lambdas — the executor doesn't know which.
pub struct Exec<N, R> {
    impl_visit_children: Arc<ChildVisitorFn<N, R>>,
}

impl<N, R> Clone for Exec<N, R> {
    fn clone(&self) -> Self { Exec { impl_visit_children: self.impl_visit_children.clone() } }
}

// --- Constructors: each checks its own bounds ---

impl<N: 'static, R: 'static> Exec<N, R> {
    /// Fused: callback-based traversal. Zero allocation.
    /// Recursion and accumulation interleave inside graph.visit.
    pub fn fused() -> Self {
        Exec { impl_visit_children: Arc::new(|graph, node, recurse, handle| {
            graph.visit(node, &mut |child: &N| handle(&recurse(child)));
        })}
    }

    /// Custom child visitor.
    pub fn new(impl_visit_children: Arc<ChildVisitorFn<N, R>>) -> Self {
        Exec { impl_visit_children }
    }

    /// Transform the child visitor.
    pub fn map_impl_visit_children(
        &self,
        mapper: impl FnOnce(Arc<ChildVisitorFn<N, R>>) -> Arc<ChildVisitorFn<N, R>>,
    ) -> Self {
        Exec { impl_visit_children: mapper(self.impl_visit_children.clone()) }
    }

    pub fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        run_inner(&self.impl_visit_children, fold, graph, root)
    }
}

impl<N: Clone + 'static, R: 'static> Exec<N, R> {
    /// Unfused sequential: collect children, process one by one.
    pub fn sequential() -> Self {
        Exec { impl_visit_children: Arc::new(|graph, node, recurse, handle| {
            for child in graph.apply(node) { handle(&recurse(&child)); }
        })}
    }
}

impl<N: Clone + Send + Sync + 'static, R: Send + Sync + 'static> Exec<N, R> {
    /// Unfused parallel: collect children, rayon par_iter.
    /// Send + Sync bounds checked here, encapsulated in the lambda.
    pub fn rayon() -> Self {
        Exec { impl_visit_children: Arc::new(|graph, node, recurse, handle| {
            use rayon::prelude::*;
            let children = graph.apply(node);
            if children.len() <= 1 {
                for child in &children { handle(&recurse(child)); }
            } else {
                let results: Vec<R> = children.par_iter().map(|c| recurse(c)).collect();
                for r in &results { handle(r); }
            }
        })}
    }
}

// --- Internal: the single run implementation ---

fn run_inner<N: 'static, H: 'static, R: 'static>(
    vc: &Arc<ChildVisitorFn<N, R>>,
    fold: &Fold<N, H, R>,
    graph: &Treeish<N>,
    node: &N,
) -> R {
    // Stack-allocated closure — Send + Sync because captures are Arc-based.
    let vc_c = vc.clone();
    let f_c = fold.clone();
    let g_c = graph.clone();
    let recurse = move |child: &N| -> R {
        run_inner(&vc_c, &f_c, &g_c, child)
    };

    let mut heap = fold.init(node);
    vc(graph, node, &recurse, &mut |r: &R| fold.accumulate(&mut heap, r));
    fold.finalize(&heap)
}
