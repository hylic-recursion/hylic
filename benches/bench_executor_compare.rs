//! Focused benchmark: Hylomorphic vs Pool vs Rayon (Shared domain).
//! Includes hand-rolled baselines for direct overhead comparison.

#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use hylic::domain::shared as dom;
use hylic::cata::exec::{PoolIn, PoolSpec, HylomorphicIn, HylomorphicSpec};
use hylic::prelude::{WorkPool, WorkPoolSpec, PoolExecView};

use support::scenario::{self, Scale, PreparedScenario};
use support::tree::NodeId;
use support::work::WorkSpec;

fn bench_executor_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("executor-compare");

    for def in scenario::all_scenarios(Scale::from_env()) {
        let s = PreparedScenario::from_def(&def, "sm");

        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let pool_exec = PoolIn::<hylic::domain::Shared>::new(pool, PoolSpec::default_for(3));
            let hylo_exec = HylomorphicIn::<hylic::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));

            group.bench_with_input(
                BenchmarkId::new("hylic.rayon.shared", &s.name),
                &(), |b, _| b.iter(|| black_box(dom::RAYON.run(&s.fold, &s.treeish, &s.root))),
            );

            group.bench_with_input(
                BenchmarkId::new("hylic.pool.shared", &s.name),
                &(), |b, _| b.iter(|| black_box(pool_exec.run(&s.fold, &s.treeish, &s.root))),
            );

            group.bench_with_input(
                BenchmarkId::new("hylic.hylo.shared", &s.name),
                &(), |b, _| b.iter(|| black_box(hylo_exec.run(&s.fold, &s.treeish, &s.root))),
            );

            group.bench_with_input(
                BenchmarkId::new("hand.rayon", &s.name),
                &(), |b, _| b.iter(|| black_box(handrolled_rayon(&s))),
            );

            let work = std::sync::Arc::new(s.work.clone());
            group.bench_with_input(
                BenchmarkId::new("hand.pool", &s.name),
                &(), |b, _| b.iter(|| black_box(handrolled_pool(&s.children, &work, pool, s.root))),
            );
        });
    }

    group.finish();
}

fn handrolled_rayon(s: &PreparedScenario) -> u64 {
    use rayon::prelude::*;
    fn recurse(children: &std::sync::Arc<Vec<Vec<NodeId>>>, work: &WorkSpec, node: NodeId) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        let ch = &children[node];
        if ch.len() <= 1 {
            for &child in ch { work.do_accumulate(&mut heap, &recurse(children, work, child)); }
        } else {
            let results: Vec<u64> = ch.par_iter().map(|&c| recurse(children, work, c)).collect();
            for r in &results { work.do_accumulate(&mut heap, r); }
        }
        work.do_finalize(&heap)
    }
    recurse(&s.children, &s.work, s.root)
}

fn handrolled_pool(
    children: &std::sync::Arc<Vec<Vec<NodeId>>>,
    work: &std::sync::Arc<WorkSpec>,
    pool: &std::sync::Arc<WorkPool>,
    root: NodeId,
) -> u64 {
    let view = PoolExecView::new(pool);
    fn recurse(children: &[Vec<NodeId>], work: &WorkSpec, view: &PoolExecView, node: NodeId) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        let ch = &children[node];
        if ch.len() <= 1 {
            for &child in ch { work.do_accumulate(&mut heap, &recurse(children, work, view, child)); }
        } else {
            let mid = ch.len() / 2;
            let (left, right) = view.join(
                || ch[..mid].iter().map(|&c| recurse(children, work, view, c)).collect::<Vec<_>>(),
                || ch[mid..].iter().map(|&c| recurse(children, work, view, c)).collect::<Vec<_>>(),
            );
            for r in left.iter().chain(right.iter()) { work.do_accumulate(&mut heap, r); }
        }
        work.do_finalize(&heap)
    }
    recurse(&children, &work, &view, root)
}

criterion_group!(benches, bench_executor_compare);
criterion_main!(benches);
