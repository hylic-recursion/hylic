//! Hylomorphic parallel executor: arena-based tree-aware scheduling.

pub mod arena;
mod scheduler;

use std::marker::PhantomData;
use std::sync::Arc;
use crate::ops::LiftOps;
use crate::domain::Domain;
use crate::prelude::parallel::pool::{WorkPool, PoolExecView};
use super::super::Executor;

pub struct HylomorphicSpec { pub _reserved: () }
impl HylomorphicSpec {
    pub fn default_for(_n_workers: usize) -> Self { HylomorphicSpec { _reserved: () } }
}

pub struct HylomorphicIn<D> {
    pool: Arc<WorkPool>,
    _spec: HylomorphicSpec,
    _domain: PhantomData<D>,
}

impl<D> HylomorphicIn<D> {
    pub fn new(pool: &Arc<WorkPool>, spec: HylomorphicSpec) -> Self {
        HylomorphicIn { pool: pool.clone(), _spec: spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>> Executor<N, R, D> for HylomorphicIn<D>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        let view = PoolExecView::new(&self.pool);
        scheduler::run_fold(fold, graph, root, &view)
    }
}

impl<D> HylomorphicIn<D> {
    pub fn run<N, H, R>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N>, N: Clone + Send + 'static, H: Send + 'static, R: Clone + Send + 'static {
        let view = PoolExecView::new(&self.pool);
        scheduler::run_fold(fold, graph, root, &view)
    }

    pub fn run_lifted<N, R, N0, H0, R0, H>(
        &self, lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>, graph: &<D as Domain<N0>>::Treeish, root: &N0,
    ) -> R0 where
        D: Domain<N> + Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
        N: Clone + Send + 'static, H: Send + 'static, R: Clone + Send + 'static,
        N0: Clone + Send + 'static, H0: 'static, R0: 'static,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }
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
    fn with_lift_lazy() {
        use crate::prelude::ParLazy;
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run_lifted(&ParLazy::lift(pool), &fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn with_lift_eager() {
        use crate::prelude::{ParEager, EagerSpec};
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &fold, &graph, &tree), expected);
        });
    }
}
