#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use hylic::prelude::{WorkPool, WorkPoolSpec};
use support::scenario::{self, Scale, PreparedScenario};
use support::hylic_runners::{HYLIC_MODES, run_hylic_mode};
use support::hand_runners;

fn bench_vs_handrolled(c: &mut Criterion) {
    let mut group = c.benchmark_group("overhead");

    for def in scenario::all_scenarios(Scale::Small) {
        let s = PreparedScenario::from_def(&def, "sm");
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            // Handrolled baselines
            for name in &hand_runners::HAND_MODES {
                group.bench_with_input(
                    BenchmarkId::new(*name, &s.name),
                    &(),
                    |b, _| { b.iter(|| black_box(hand_runners::run_hand(name, &s, pool))); },
                );
            }
            // All 6 hylic modes
            for mode in &HYLIC_MODES {
                group.bench_with_input(
                    BenchmarkId::new(*mode, &s.name),
                    &(),
                    |b, _| { b.iter(|| black_box(run_hylic_mode(mode, &s.fold, &s.treeish, &s.root, pool))); },
                );
            }
        });
    }

    group.finish();
}

criterion_group!(benches, bench_vs_handrolled);
criterion_main!(benches);
