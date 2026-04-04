//! Focused benchmark: Hylomorphic vs Rayon — the three that matter.
//!
//! Three modes only: hylic.rayon.shared, hand.rayon, hylic.hylo.shared
//! Subset of scenarios for fast iteration.

#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::sync::Arc;
use hylic::domain::shared as dom;
use hylic::cata::exec::{HylomorphicIn, HylomorphicSpec};
use hylic::prelude::{WorkPool, WorkPoolSpec};

use support::scenario::{Scale, PreparedScenario, ScenarioDef};
use support::tree::{NodeId, TreeSpec};
use support::work::WorkSpec;

/// Targeted scenario subset — representative workloads, not exhaustive.
fn hylo_scenarios(scale: Scale) -> Vec<ScenarioDef> {
    let (n, n_large) = match scale {
        Scale::Small => (200, 500),
        Scale::Large => (2000, 5000),
    };
    let w = |init, acc, fin, graph, io| WorkSpec {
        init_work: init, accumulate_work: acc, finalize_work: fin,
        graph_work: graph, graph_io_us: io,
    };
    vec![
        ScenarioDef { name: "noop",       moniker: "noop",     tree: TreeSpec { node_count: n, branch_factor: 8 },  work: w(0, 0, 0, 0, 0) },
        ScenarioDef { name: "parse-heavy", moniker: "parse-hv", tree: TreeSpec { node_count: n, branch_factor: 8 },  work: w(200_000, 10_000, 10_000, 50_000, 0) },
        ScenarioDef { name: "balanced",    moniker: "bal",      tree: TreeSpec { node_count: n, branch_factor: 8 },  work: w(50_000, 50_000, 50_000, 50_000, 0) },
        ScenarioDef { name: "graph-heavy", moniker: "graph-hv", tree: TreeSpec { node_count: n, branch_factor: 8 },  work: w(5_000, 10_000, 5_000, 200_000, 0) },
        ScenarioDef { name: "wide-shallow",moniker: "wide",     tree: TreeSpec { node_count: n, branch_factor: 20 }, work: w(50_000, 10_000, 10_000, 10_000, 0) },
        ScenarioDef { name: "deep-narrow", moniker: "deep",     tree: TreeSpec { node_count: n, branch_factor: 2 },  work: w(50_000, 10_000, 10_000, 10_000, 0) },
        ScenarioDef { name: "large-dense", moniker: "lg-dense", tree: TreeSpec { node_count: n_large, branch_factor: 10 }, work: w(50_000, 10_000, 10_000, 10_000, 0) },
    ]
}

fn bench_hylo_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("hylo-compare");

    for def in hylo_scenarios(Scale::from_env()) {
        let s = PreparedScenario::from_def(&def, "sm");

        // hylic.rayon.shared
        group.bench_with_input(
            BenchmarkId::new("hylic.rayon", &s.name), &(),
            |b, _| b.iter(|| black_box(dom::RAYON.run(&s.fold, &s.treeish, &s.root))),
        );

        // hand.rayon
        group.bench_with_input(
            BenchmarkId::new("hand.rayon", &s.name), &(),
            |b, _| b.iter(|| black_box(handrolled_rayon(&s))),
        );

        // hylic.hylo.shared
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let hylo = HylomorphicIn::<hylic::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            group.bench_with_input(
                BenchmarkId::new("hylic.hylo", &s.name), &(),
                |b, _| b.iter(|| black_box(hylo.run(&s.fold, &s.treeish, &s.root))),
            );
        });
    }
    group.finish();
}

fn handrolled_rayon(s: &PreparedScenario) -> u64 {
    use rayon::prelude::*;
    fn recurse(children: &Arc<Vec<Vec<NodeId>>>, work: &WorkSpec, node: NodeId) -> u64 {
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

criterion_group!(benches, bench_hylo_compare);
criterion_main!(benches);
