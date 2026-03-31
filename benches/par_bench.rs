mod bench_support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use bench_support::*;
use hylic::prelude::{WorkPool, WorkPoolSpec};

fn bench_executors(c: &mut Criterion) {
    let mut group = c.benchmark_group("executors");
    for cfg in all_configs() {
        let (graph, fold, root) = prepare(&cfg);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            for name in &MODE_NAMES {
                group.bench_with_input(
                    BenchmarkId::new(*name, cfg.name),
                    &(),
                    |b, _| { b.iter(|| black_box(run_mode(name, &fold, &graph, &root, pool))); },
                );
            }
        });
    }
    group.finish();
}

criterion_group!(benches, bench_executors);
criterion_main!(benches);
