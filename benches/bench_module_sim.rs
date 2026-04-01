#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use hylic::prelude::{WorkPool, WorkPoolSpec};
use support::module_sim;

fn bench_module_sim(c: &mut Criterion) {
    let mut group = c.benchmark_group("module-sim");

    for spec in module_sim::all_module_scenarios(false) {
        let sim = module_sim::prepare(&spec);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let modes = module_sim::build_all(&sim, pool);
            for mode in &modes {
                group.bench_with_input(
                    BenchmarkId::new(mode.name, &sim.name),
                    &(),
                    |b, _| b.iter(|| black_box((mode.run)())),
                );
            }
        });
    }

    group.finish();
}

criterion_group!(benches, bench_module_sim);
criterion_main!(benches);
