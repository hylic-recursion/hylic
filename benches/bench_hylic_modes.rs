#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use hylic::prelude::{WorkPool, WorkPoolSpec};
use support::scenario::{self, Scale, PreparedScenario};
use support::hylic_runners;

fn bench_hylic_modes(c: &mut Criterion) {
    let mut group = c.benchmark_group("hylic-modes");

    for def in scenario::all_scenarios(Scale::Small) {
        let s = PreparedScenario::from_def(&def, "sm");
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let modes = hylic_runners::all_modes(pool);
            for mode in &modes {
                group.bench_with_input(
                    BenchmarkId::new(mode.name, &s.name),
                    &(),
                    |b, _| { b.iter(|| black_box(hylic_runners::run(mode, &s))); },
                );
            }
        });
    }

    group.finish();
}

criterion_group!(benches, bench_hylic_modes);
criterion_main!(benches);
