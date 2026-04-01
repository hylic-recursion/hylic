#[path = "support/mod.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use hylic::prelude::{WorkPool, WorkPoolSpec};
use support::scenario::{self, Scale, PreparedScenario};
use support::{hylic_runners, hand_runners};

fn bench_vs_handrolled(c: &mut Criterion) {
    let mut group = c.benchmark_group("overhead");

    for def in scenario::all_scenarios(Scale::Small) {
        let s = PreparedScenario::from_def(&def, "sm");
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let hand_modes = hand_runners::build_all(&s, pool);
            let hylic_modes = hylic_runners::build_all(&s.fold, &s.treeish, &s.root, pool);

            for mode in hand_modes.iter().chain(hylic_modes.iter()) {
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

criterion_group!(benches, bench_vs_handrolled);
criterion_main!(benches);
