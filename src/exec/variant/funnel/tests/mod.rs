//! Funnel test suite — same rigor as hylo, same test patterns.
//!
//! Every test runs for both queue strategies via policies:
//!   policy::Default (PerWorker + OnFinalize + EveryPush)
//!   policy::SharedDefault (Shared + OnFinalize + EveryPush)

mod api;
mod correctness;
mod stress;
mod interleaving;

use crate::domain::shared as dom;
use crate::exec::ExecutorSpec;
use super::{Spec, Pool};
use super::policy::{self, FunnelPolicy};
use super::policy::queue::WorkStealing;
use super::policy::accumulate::AccumulateStrategy;
use super::policy::wake::WakeStrategy;

pub(super) fn n_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

// ── Shared tree types (same as hylo tests) ───────────

#[derive(Clone)]
pub(super) struct N {
    pub val: i32,
    pub children: Vec<N>,
}

pub(super) fn big_tree(n: usize, bf: usize) -> N {
    if n == 0 { return N { val: 0, children: vec![] }; }
    let mut nodes: Vec<N> = (0..n).map(|i| N { val: (i + 1) as i32, children: vec![] }).collect();
    for i in (0..n).rev() {
        let first_child = i * bf + 1;
        if first_child < n {
            let last_child = (first_child + bf).min(n);
            let children: Vec<N> = (first_child..last_child)
                .rev()
                .map(|c| std::mem::replace(&mut nodes[c], N { val: 0, children: vec![] }))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            nodes[i].children = children;
        }
    }
    nodes.into_iter().next().unwrap()
}

pub(super) fn gen_adj(node_count: usize, bf: usize) -> std::sync::Arc<Vec<Vec<usize>>> {
    let mut children: Vec<Vec<usize>> = vec![vec![]];
    let mut next_id = 1usize;
    let mut level_start = 0;
    let mut level_end = 1;
    while next_id < node_count {
        let mut new_end = level_end;
        for parent in level_start..level_end {
            let n_ch = bf.min(node_count - next_id);
            if n_ch == 0 { break; }
            let mut my_ch = Vec::with_capacity(n_ch);
            for _ in 0..n_ch {
                if next_id >= node_count { break; }
                children.push(vec![]);
                my_ch.push(next_id);
                next_id += 1;
                new_end += 1;
            }
            children[parent] = my_ch;
        }
        level_start = level_end;
        level_end = new_end;
        if level_start == level_end { break; }
    }
    std::sync::Arc::new(children)
}

pub(super) fn sum_fold() -> dom::Fold<N, i32, i32> {
    dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; })
}

pub(super) fn n_graph() -> crate::graph::Treeish<N> {
    crate::graph::treeish(|n: &N| n.children.clone())
}

// ── Policy-generic helpers ───────────────────────────

/// Run a fold on a pre-existing pool with the given policy.
pub(super) fn run_on_pool<P: FunnelPolicy, R>(
    pool: &Pool<'_>,
    n_workers: usize,
    f: impl for<'s> FnOnce(&crate::exec::Exec<crate::domain::Shared, super::Session<'s, P>>) -> R,
) -> R {
    let spec = Spec::<P>::new(
            n_workers,
            <P::Queue as WorkStealing>::Spec::default(),
            <P::Accumulate as AccumulateStrategy>::Spec::default(),
            <P::Wake as WakeStrategy>::Spec::default(),
        );
    let exec = dom::exec(spec.attach(pool));
    f(&exec)
}

/// One-shot: create pool, run fold, destroy pool.
pub(super) fn with_exec<P: FunnelPolicy, R>(
    n_workers: usize,
    f: impl for<'s> FnOnce(&crate::exec::Exec<crate::domain::Shared, super::Session<'s, P>>) -> R,
) -> R {
    Pool::with(n_workers, |pool| run_on_pool::<P, _>(pool, n_workers, f))
}

pub(super) fn assert_matches_fused_with<P: FunnelPolicy>(tree: &N, n_workers: usize) {
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, tree);
    with_exec::<P, _>(n_workers, |exec| {
        assert_eq!(exec.run(&fold, &graph, tree), expected);
    });
}
