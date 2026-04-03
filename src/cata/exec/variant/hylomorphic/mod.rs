//! Hylomorphic parallel executor: preserves graph/fold fusion.
//!
//! Children are processed one at a time via visit_inline (like Fused).
//! For each child, the executor decides: recurse locally (fused DFS)
//! or fork to a worker (who runs the full fused hylomorphism on the
//! subtree). No Vec of children is ever materialized.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::ops::{FoldOps, TreeOps, LiftOps};
use crate::domain::Domain;
use crate::prelude::parallel::pool::{WorkPool, PoolExecView};
use crate::prelude::parallel::completion::Completion;
use super::super::Executor;

pub struct HylomorphicSpec {
    pub local_first: usize,
}

impl HylomorphicSpec {
    pub fn default_for(_n_workers: usize) -> Self {
        HylomorphicSpec { local_first: 1 }
    }
}

pub struct HylomorphicIn<D> {
    pool: Arc<WorkPool>,
    spec: HylomorphicSpec,
    _domain: PhantomData<D>,
}

impl<D> HylomorphicIn<D> {
    pub fn new(pool: &Arc<WorkPool>, spec: HylomorphicSpec) -> Self {
        HylomorphicIn { pool: pool.clone(), spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>> Executor<N, R, D> for HylomorphicIn<D>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        let view = PoolExecView::new(&self.pool);
        hylo_recurse(fold, graph, root, &view, &self.spec)
    }
}

impl<D> HylomorphicIn<D> {
    pub fn run<N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N> {
        let view = PoolExecView::new(&self.pool);
        hylo_recurse(fold, graph, root, &view, &self.spec)
    }

    pub fn run_lifted<N: Clone + Send + 'static, R: Clone + Send + 'static, N0: Clone + Send + 'static, H0: 'static, R0: 'static, H: 'static>(
        &self, lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>, graph: &<D as Domain<N0>>::Treeish, root: &N0,
    ) -> R0 where D: Domain<N> + Domain<N0>, <D as Domain<N0>>::Fold<H0, R0>: Clone, <D as Domain<N0>>::Treeish: Clone {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }
}

// ── Lifetime-erased pointers ─────────────────────
//
// For forked closures that need fold/graph access.
// SAFETY: hylo_recurse waits for ALL Completions before returning.

struct FoldRef<N, H, R>(*const dyn FoldOps<N, H, R>);
unsafe impl<N, H, R> Send for FoldRef<N, H, R> {}
impl<N, H, R> Clone for FoldRef<N, H, R> { fn clone(&self) -> Self { *self } }
impl<N, H, R> Copy for FoldRef<N, H, R> {}

struct GraphRef<N>(*const dyn TreeOps<N>);
unsafe impl<N> Send for GraphRef<N> {}
impl<N> Clone for GraphRef<N> { fn clone(&self) -> Self { *self } }
impl<N> Copy for GraphRef<N> {}

// ── Recursion engine ──────────────────────────────

struct ForkSlot<R> {
    index: usize,
    completion: Completion<R>,
}

/// Entry point: concrete types, uses visit_inline for zero dispatch.
fn hylo_recurse<N, H: 'static, R>(
    fold: &(impl FoldOps<N, H, R> + 'static),
    graph: &(impl TreeOps<N> + 'static),
    node: &N,
    view: &PoolExecView,
    spec: &HylomorphicSpec,
) -> R
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    let fold_ref = FoldRef(fold as &dyn FoldOps<N, H, R> as *const _);
    let graph_ref = GraphRef(graph as &dyn TreeOps<N> as *const _);
    hylo_core(fold, graph, fold_ref, graph_ref, node, view, spec)
}

/// Core recursion with both concrete refs (for visit_inline) and
/// erased pointers (for forking). The concrete refs are used for local
/// recursion. The erased pointers are captured by forked closures.
fn hylo_core<N, H: 'static, R>(
    fold: &(impl FoldOps<N, H, R> + ?Sized + 'static),
    graph: &(impl TreeOps<N> + ?Sized + 'static),
    fold_ref: FoldRef<N, H, R>,
    graph_ref: GraphRef<N>,
    node: &N,
    view: &PoolExecView,
    spec: &HylomorphicSpec,
) -> R
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    let mut heap = fold.init(node);
    let mut child_index = 0usize;
    let mut local_results: Vec<(usize, R)> = Vec::new();
    let mut forked: Vec<ForkSlot<R>> = Vec::new();

    let vh = view.handle();

    // visit (not visit_inline) because fold/graph might be dyn on worker path.
    graph.visit(node, &mut |child: &N| {
        let my_index = child_index;
        child_index += 1;

        if my_index < spec.local_first || !view.queue_is_empty() {
            // Local: fused hylomorphism. graph.visit dispatches per the
            // concrete or dyn type — the recursion stays fused either way.
            let r = hylo_core(fold, graph, fold_ref, graph_ref, child, view, spec);
            local_results.push((my_index, r));
        } else {
            // Fork: clone child, submit to pool.
            let child_owned = child.clone();
            let completion = Completion::new();
            let comp_clone = completion.clone();
            let vh_clone = vh.clone();
            let local_first = spec.local_first;

            vh.submit(Box::new(move || {
                // SAFETY: fold_ref/graph_ref point to caller's fold/graph.
                // Caller waits for all Completions before returning.
                let fold = unsafe { &*fold_ref.0 };
                let graph = unsafe { &*graph_ref.0 };
                let sub_view = PoolExecView::new_from_handle(&vh_clone);
                let spec = HylomorphicSpec { local_first };
                let r = hylo_core(fold, graph, fold_ref, graph_ref, &child_owned, &sub_view, &spec);
                comp_clone.set(r);
            }));

            forked.push(ForkSlot { index: my_index, completion });
        }
    });

    // Accumulate in child order.
    let total = child_index;
    if forked.is_empty() {
        for (_, r) in &local_results {
            fold.accumulate(&mut heap, r);
        }
    } else {
        let mut local_iter = local_results.iter().peekable();
        let mut forked_iter = forked.iter().peekable();
        for i in 0..total {
            if local_iter.peek().is_some_and(|(idx, _)| *idx == i) {
                let (_, r) = local_iter.next().unwrap();
                fold.accumulate(&mut heap, r);
            } else if forked_iter.peek().is_some_and(|f| f.index == i) {
                let slot = forked_iter.next().unwrap();
                let r = slot.completion.wait(vh.clone());
                fold.accumulate(&mut heap, &r);
            }
        }
    }

    fold.finalize(&heap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::shared as dom;
    use crate::prelude::{WorkPool, WorkPoolSpec};

    #[derive(Clone)]
    struct N { val: i32, children: Vec<N> }

    fn big_tree(n: usize, bf: usize) -> N {
        fn build(id: &mut i32, remaining: &mut usize, bf: usize) -> N {
            let val = *id; *id += 1; *remaining = remaining.saturating_sub(1);
            let mut ch = Vec::new();
            for _ in 0..bf { if *remaining == 0 { break; } ch.push(build(id, remaining, bf)); }
            N { val, children: ch }
        }
        let mut id = 1; let mut remaining = n;
        build(&mut id, &mut remaining, bf)
    }

    #[test]
    fn matches_fused() {
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn matches_fused_200() {
        let tree = big_tree(200, 6);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(4), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(4));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn zero_workers() {
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(0), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(0));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn stress_20x() {
        let tree = big_tree(200, 6);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        for i in 0..20 {
            WorkPool::with(WorkPoolSpec::threads(4), |pool| {
                let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(4));
                assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
            });
        }
    }

    #[test]
    fn with_lift() {
        use crate::prelude::{ParLazy, ParEager, EagerSpec};
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run_lifted(&ParLazy::lift(pool), &fold, &graph, &tree), expected, "Lazy+Hylo");
            assert_eq!(exec.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &fold, &graph, &tree), expected, "Eager+Hylo");
        });
    }

    #[test]
    fn local_domain() {
        use crate::domain::local;
        let tree = big_tree(60, 4);
        let fold = local::fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; }, |h: &i32| *h);
        let graph = local::treeish_visit(|n: &N, cb: &mut dyn FnMut(&N)| { for c in &n.children { cb(c); } });
        let expected = local::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Local>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }
}
