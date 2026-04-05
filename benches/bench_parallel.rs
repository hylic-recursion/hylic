#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use hylic::prelude::{WorkPool, WorkPoolSpec};
use support::scenario::{self, Scale, PreparedScenario};
use support::modes;
use support::bench_cell;

fn bench_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel");

    for def in scenario::all_scenarios(Scale::from_env()) {
        let s = PreparedScenario::from_def(&def, "sm");
        let nw = support::config::bench_workers();
        WorkPool::with(WorkPoolSpec::threads(nw), |pool| {
            let modes = modes::parallel_modes(&s, pool);
            for mode in &modes {
                bench_cell(&mut group, mode.name, &s.name,
                    |b, _| b.iter(|| black_box((mode.run)())),
                );
            }
        });
    }

    group.finish();
}

criterion_group!(benches, bench_parallel);
criterion_main!(benches);
