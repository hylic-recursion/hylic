use std::sync::Arc;
use crate::graph::types::Treeish;
use crate::fold::Fold;
use super::Lift;

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

/// Unified executor. Fused mode uses a specialized zero-Arc path;
/// all other modes use a ChildVisitorFn lambda.
pub struct Exec<N, R> {
    inner: ExecInner<N, R>,
}

enum ExecInner<N, R> {
    /// Zero-overhead recursive traversal via direct reference passing.
    /// No Arc clones per recursion — fold and graph are borrowed.
    Fused,
    /// General-purpose: child-visiting lambda encapsulates traversal
    /// mode and parallelism.
    Custom(Arc<ChildVisitorFn<N, R>>),
}

impl<N, R> Clone for Exec<N, R> {
    fn clone(&self) -> Self {
        Exec { inner: match &self.inner {
            ExecInner::Fused => ExecInner::Fused,
            ExecInner::Custom(f) => ExecInner::Custom(f.clone()),
        }}
    }
}

// --- Constructors ---

impl<N: 'static, R: 'static> Exec<N, R> {
    /// Fused: callback-based traversal. Zero allocation, zero Arc clones.
    /// Recursion and accumulation interleave inside graph.visit.
    pub fn fused() -> Self {
        Exec { inner: ExecInner::Fused }
    }

    /// Custom child visitor.
    pub fn new(impl_visit_children: Arc<ChildVisitorFn<N, R>>) -> Self {
        Exec { inner: ExecInner::Custom(impl_visit_children) }
    }

    pub fn run<H: 'static>(&self, fold: &Fold<N, H, R>, graph: &Treeish<N>, root: &N) -> R {
        match &self.inner {
            ExecInner::Fused => run_inner_fused(fold, graph, root),
            ExecInner::Custom(vc) => run_inner(vc, fold, graph, root),
        }
    }

    // ANCHOR: run_lifted
    /// Run a lifted computation: transform fold + treeish via the Lift,
    /// execute with this executor, unwrap the result.
    pub fn run_lifted<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &Lift<N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> R0 {
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_root = lift.lift_root(root);
        let inner_result = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        lift.unwrap(inner_result)
    }
    // ANCHOR_END: run_lifted

    /// Run lifted, returning both the unwrapped result and the lifted result.
    pub fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &Lift<N0, H0, R0, N, H, R>,
        fold: &Fold<N0, H0, R0>,
        graph: &Treeish<N0>,
        root: &N0,
    ) -> (R0, R) where R: Clone {
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_root = lift.lift_root(root);
        let inner_result = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        let unwrapped = lift.unwrap(inner_result.clone());
        (unwrapped, inner_result)
    }
}

impl<N: Clone + 'static, R: 'static> Exec<N, R> {
    /// Unfused sequential: collect children, process one by one.
    pub fn sequential() -> Self {
        Exec::new(Arc::new(|graph, node, recurse, handle| {
            for child in graph.apply(node) { handle(&recurse(&child)); }
        }))
    }
}

impl<N: Clone + Send + Sync + 'static, R: Send + Sync + 'static> Exec<N, R> {
    /// Unfused parallel: collect children, rayon par_iter.
    /// Send + Sync bounds checked here, encapsulated in the lambda.
    pub fn rayon() -> Self {
        Exec::new(Arc::new(|graph, node, recurse, handle| {
            use rayon::prelude::*;
            let children = graph.apply(node);
            if children.len() <= 1 {
                for child in &children { handle(&recurse(child)); }
            } else {
                let results: Vec<R> = children.par_iter().map(|c| recurse(c)).collect();
                for r in &results { handle(r); }
            }
        }))
    }
}

// --- Fused: zero-Arc recursive traversal ---

// ANCHOR: run_inner_fused
fn run_inner_fused<N: 'static, H: 'static, R: 'static>(
    fold: &Fold<N, H, R>,
    graph: &Treeish<N>,
    node: &N,
) -> R {
    let mut heap = fold.init(node);
    graph.visit(node, &mut |child: &N| {
        let r = run_inner_fused(fold, graph, child);
        fold.accumulate(&mut heap, &r);
    });
    fold.finalize(&heap)
}
// ANCHOR_END: run_inner_fused

// --- Custom: generic path with ChildVisitorFn ---

// ANCHOR: run_inner
fn run_inner<N: 'static, H: 'static, R: 'static>(
    vc: &Arc<ChildVisitorFn<N, R>>,
    fold: &Fold<N, H, R>,
    graph: &Treeish<N>,
    node: &N,
) -> R {
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
// ANCHOR_END: run_inner
