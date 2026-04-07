//! Focused benchmark: Hylomorphic vs Rayon — all executor variants.
//!
//! Uses only the public executor API (HyloFunnelIn::run).
//! No internal FoldContext or run_fold_with — allocation is part of
//! the measurement, same as hylo and rayon.
//!
//! Naming: rayon, hand.rayon, hylo, funnel.pw.arrive, funnel.pw.final,
//!         funnel.sh.arrive, funnel.sh.final
//!
//! 6 scenarios: noop, wide-light, fold-light, graph-heavy, fold-heavy, bal-heavy.

#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use std::sync::Arc;
use hylic::domain::shared as dom;
use hylic::cata::exec::{HylomorphicIn, HylomorphicSpec, HyloFunnelIn, HyloFunnelSpec, AccumulateMode};
use hylic::cata::exec::variant::hylo_funnel::queue;
use hylic::prelude::{WorkPool, WorkPoolSpec};

use support::scenario::{Scale, PreparedScenario, ScenarioDef};
use support::tree::{NodeId, TreeSpec};
use support::work::WorkSpec;
use support::config;
use support::bench_cell;

fn hylo_scenarios(_scale: Scale) -> Vec<ScenarioDef> {
    let w = |init, acc, fin, graph, io| WorkSpec {
        init_work: init, accumulate_work: acc, finalize_work: fin,
        graph_work: graph, graph_io_us: io,
    };
    vec![
        ScenarioDef { name: "noop",        moniker: "noop",     tree: TreeSpec { node_count: 200, branch_factor: 8 },  work: w(0, 0, 0, 0, 0) },
        ScenarioDef { name: "wide-light",  moniker: "wide-lt",  tree: TreeSpec { node_count: 80, branch_factor: 20 },  work: w(50_000, 10_000, 10_000, 10_000, 0) },
        ScenarioDef { name: "fold-light",  moniker: "fold-lt",  tree: TreeSpec { node_count: 80, branch_factor: 8 },   work: w(50_000, 50_000, 50_000, 5_000, 0) },
        ScenarioDef { name: "graph-heavy", moniker: "graph-hv", tree: TreeSpec { node_count: 500, branch_factor: 8 },  work: w(5_000, 5_000, 5_000, 500_000, 0) },
        ScenarioDef { name: "fold-heavy",  moniker: "fold-hv",  tree: TreeSpec { node_count: 500, branch_factor: 8 },  work: w(200_000, 200_000, 200_000, 5_000, 0) },
        ScenarioDef { name: "bal-heavy",   moniker: "bal-hv",   tree: TreeSpec { node_count: 500, branch_factor: 8 },  work: w(100_000, 100_000, 100_000, 100_000, 0) },
    ]
}

fn bench_hylo_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("hylo-compare");
    let nw = config::bench_workers();
    eprintln!("[hylo-compare] using {nw} worker threads");

    for def in hylo_scenarios(Scale::from_env()) {
        let s = PreparedScenario::from_def(&def, "sm");

        // ── Rayon baselines ─────────────────────────
        bench_cell(&mut group, "rayon", &s.name,
            |b, _| b.iter(|| black_box(dom::RAYON.run(&s.fold, &s.treeish, &s.root))),
        );
        bench_cell(&mut group, "hand.rayon", &s.name,
            |b, _| b.iter(|| black_box(handrolled_rayon(&s))),
        );

        // ── Hylo (baseline hylomorphic executor) ────
        WorkPool::with(WorkPoolSpec::threads(nw), |pool| {
            let hylo = HylomorphicIn::<hylic::domain::Shared>::new(pool, HylomorphicSpec::default_for(nw));
            bench_cell(&mut group, "hylo", &s.name,
                |b, _| b.iter(|| black_box(hylo.run(&s.fold, &s.treeish, &s.root))),
            );
        });

        // ── Funnel: PerWorker × OnArrival ───────────
        {
            let exec = HyloFunnelIn::<hylic::domain::Shared, queue::PerWorker>::new(
                nw, HyloFunnelSpec::per_worker(AccumulateMode::OnArrival),
            );
            bench_cell(&mut group, "funnel.pw.arrive", &s.name,
                |b, _| b.iter(|| black_box(exec.run(&s.fold, &s.treeish, &s.root))),
            );
        }

        // ── Funnel: PerWorker × OnFinalize ──────────
        {
            let exec = HyloFunnelIn::<hylic::domain::Shared, queue::PerWorker>::new(
                nw, HyloFunnelSpec::per_worker(AccumulateMode::OnFinalize),
            );
            bench_cell(&mut group, "funnel.pw.final", &s.name,
                |b, _| b.iter(|| black_box(exec.run(&s.fold, &s.treeish, &s.root))),
            );
        }

        // ── Funnel: Shared × OnArrival ──────────────
        {
            let exec = HyloFunnelIn::<hylic::domain::Shared, queue::Shared>::new(
                nw, HyloFunnelSpec::shared(AccumulateMode::OnArrival),
            );
            bench_cell(&mut group, "funnel.sh.arrive", &s.name,
                |b, _| b.iter(|| black_box(exec.run(&s.fold, &s.treeish, &s.root))),
            );
        }

        // ── Funnel: Shared × OnFinalize ─────────────
        {
            let exec = HyloFunnelIn::<hylic::domain::Shared, queue::Shared>::new(
                nw, HyloFunnelSpec::shared(AccumulateMode::OnFinalize),
            );
            bench_cell(&mut group, "funnel.sh.final", &s.name,
                |b, _| b.iter(|| black_box(exec.run(&s.fold, &s.treeish, &s.root))),
            );
        }
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
