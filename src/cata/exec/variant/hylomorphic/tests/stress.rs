//! Stress tests: repeated execution, pool lifecycle churn, hang detection.

use super::*;

/// 200 pool create/destroy cycles, each with a full 200-node fold.
/// Guards: condvar lost-wakeup on pool shutdown (the bug that hung here).
/// Guards: fold correctness under rapid pool churn.
#[test]
fn stress_200_pools() {
    let tree = big_tree(200, 6);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    for i in 0..200 {
        WorkPool::with(WorkPoolSpec::threads(nt), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(nt));
            assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
        });
    }
}

/// 50k folds on ONE pool. Adjacency-list, noop fold, full thread count.
/// Guards: raker stranding bug (now fixed by ticket-based on-arrival sweep).
#[test]
fn stress_50k_runs_one_pool() {
    let adj = gen_adj(200, 8);
    let ch = adj.clone();
    let treeish = dom::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &child in &ch[*n] { cb(&child); }
    });
    let fold = dom::fold(
        |_: &usize| 0u64,
        |h: &mut u64, c: &u64| { *h += c; },
        |h: &u64| *h,
    );
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    let nt = n_threads();
    WorkPool::with(WorkPoolSpec::threads(nt), |pool| {
        let exec = HylomorphicIn::<crate::domain::Shared>::new(
            pool, HylomorphicSpec::default_for(nt));
        for i in 0..50_000 {
            let result = exec.run(&fold, &treeish, &0usize);
            assert_eq!(result, expected, "iteration {i}");
        }
    });
}

/// 500 pool create/destroy cycles with ZERO work.
/// Guards: condvar lost-wakeup in pool shutdown path, isolated from fold logic.
/// This is the pure pool lifecycle regression test.
#[test]
fn pool_lifecycle_500() {
    let nt = n_threads();
    for _ in 0..500 {
        WorkPool::with(WorkPoolSpec::threads(nt), |_pool| {});
    }
}

/// 500 pool create/destroy cycles with trivial submit+drain.
/// Guards: condvar wakeup for task notification + shutdown interaction.
#[test]
fn pool_lifecycle_with_work_500() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let nt = n_threads();
    for _ in 0..500 {
        WorkPool::with(WorkPoolSpec::threads(nt), |pool| {
            let view = crate::prelude::PoolExecView::new(pool);
            let handle = view.handle();
            let done = Arc::new(AtomicBool::new(false));
            let d2 = done.clone();
            handle.submit(move || { d2.store(true, Ordering::Release); });
            while !done.load(Ordering::Acquire) {
                if !view.help_once() { std::hint::spin_loop(); }
            }
        });
    }
}
