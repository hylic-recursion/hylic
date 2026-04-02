#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use hylic::prelude::{WorkPool, WorkPoolSpec};
use support::scenario::{self, Scale, PreparedScenario};
use support::modes;

fn bench_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel");

    for def in scenario::all_scenarios(Scale::from_env()) {
        let s = PreparedScenario::from_def(&def, "sm");
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let modes = modes::parallel_modes(&s, pool);
            for mode in &modes {
                group.bench_with_input(
                    BenchmarkId::new(mode.name, &s.name),
                    &(),
                    |b, _| b.iter(|| black_box((mode.run)())),
                );
            }
        });
    }

    group.finish();
}

criterion_group!(benches, bench_parallel);
criterion_main!(benches);
