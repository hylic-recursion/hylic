#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use support::scenario::{self, Scale, PreparedScenario};
use support::modes;

fn bench_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential");

    for def in scenario::all_scenarios(Scale::from_env()) {
        let s = PreparedScenario::from_def(&def, "sm");
        let modes = modes::sequential_modes(&s);
        for mode in &modes {
            group.bench_with_input(
                BenchmarkId::new(mode.name, &s.name),
                &(),
                |b, _| b.iter(|| black_box((mode.run)())),
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_sequential);
criterion_main!(benches);
