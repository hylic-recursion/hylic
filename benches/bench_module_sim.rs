#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use hylic::prelude::{WorkPool, WorkPoolSpec};
use support::module_sim;
use support::bench_cell;

fn bench_module_sim(c: &mut Criterion) {
    let mut group = c.benchmark_group("module-sim");

    for spec in module_sim::all_module_scenarios(false) {
        let sim = module_sim::prepare(&spec);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            module_sim::with_all_modes(&sim, pool, |modes| {
                for mode in modes {
                    bench_cell(&mut group, mode.name, &sim.name,
                        |b, _| b.iter(|| black_box((mode.run)())),
                    );
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_module_sim);
criterion_main!(benches);
