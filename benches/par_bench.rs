mod bench_support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::sync::Arc;

use hylic::graph::treeish;
use hylic::fold;
use hylic::cata::Exec;
use hylic::prelude::uio_parallel;
use bench_support::*;

// ── Workload configs ────────────────────────────────────────
//
// NodeId = usize — O(1) clone. Tree structure is external.
// Work levels calibrated so parallelism has real savings to show.

struct BenchConfig {
    name: &'static str,
    tree: TreeSpec,
    graph_latency_us: u64,
    graph_compute: u64,     // busy_work iterations in graph
    fold_compute: u64,      // busy_work iterations in fold
}

fn configs() -> Vec<BenchConfig> {
    vec![
        BenchConfig {
            name: "0us:overhead",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 0, fold_compute: 0,
        },
        BenchConfig {
            name: "10us:light",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 5_000, fold_compute: 5_000,
        },
        BenchConfig {
            name: "100us:graph-heavy",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 100_000, fold_compute: 5_000,
        },
        BenchConfig {
            name: "100us:fold-heavy",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 5_000, fold_compute: 100_000,
        },
        BenchConfig {
            name: "200us:io",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 200, graph_compute: 0, fold_compute: 5_000,
        },
        BenchConfig {
            name: "200us:balanced",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 100_000, fold_compute: 100_000,
        },
        BenchConfig {
            name: "1ms:heavy",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 500_000, fold_compute: 500_000,
        },
        BenchConfig {
            name: "200us:deep",
            tree: TreeSpec { node_count: 200, branch_factor: 2 },
            graph_latency_us: 200, graph_compute: 0, fold_compute: 5_000,
        },
        BenchConfig {
            name: "100us:large500",
            tree: TreeSpec { node_count: 500, branch_factor: 10 },
            graph_latency_us: 0, graph_compute: 50_000, fold_compute: 50_000,
        },
    ]
}

// ── Execution modes: the 2×2 matrix ────────────────────────

enum Mode { Fused, Rayon, UioFused, UioRayon }
impl Mode {
    fn name(&self) -> &'static str {
        match self {
            Mode::Fused => "fused",
            Mode::Rayon => "rayon",
            Mode::UioFused => "uio+fused",
            Mode::UioRayon => "uio+rayon",
        }
    }
}

// ── Benchmark ───────────────────────────────────────────────

fn bench_executors(c: &mut Criterion) {
    let mut group = c.benchmark_group("executors");

    for cfg in configs() {
        // Setup: build tree and graph OUTSIDE the benchmark loop.
        let tree = gen_tree(&cfg.tree, 42);
        let gl = cfg.graph_latency_us;
        let gc = cfg.graph_compute;
        let fc = cfg.fold_compute;

        let children = Arc::new(tree.children);
        let graph = {
            let ch = children.clone();
            treeish(move |n: &NodeId| {
                spin_wait_us(gl);
                if gc > 0 { black_box(busy_work(gc)); }
                ch[*n].clone() // Vec<usize> — cheap
            })
        };

        let my_fold = fold::simple_fold(
            move |_n: &NodeId| {
                if fc > 0 { busy_work(fc) } else { 0u64 }
            },
            |a: &mut u64, c: &u64| { *a = a.wrapping_add(*c); },
        );

        let root = tree.root;
        let modes = [Mode::Fused, Mode::Rayon, Mode::UioFused, Mode::UioRayon];

        for mode in &modes {
            group.bench_with_input(
                BenchmarkId::new(mode.name(), cfg.name),
                &(),
                |b, _| match mode {
                    Mode::Fused => {
                        let exec = Exec::fused();
                        b.iter(|| exec.run(&my_fold, &graph, black_box(&root)));
                    }
                    Mode::Rayon => {
                        let exec = Exec::rayon();
                        b.iter(|| exec.run(&my_fold, &graph, black_box(&root)));
                    }
                    Mode::UioFused => {
                        let exec = Exec::fused();
                        let lift = uio_parallel();
                        b.iter(|| exec.run_lifted(&my_fold, &graph, black_box(&root), &lift));
                    }
                    Mode::UioRayon => {
                        let exec = Exec::fused();
                        let lift = uio_parallel().with_inner_exec(|| Exec::rayon());
                        b.iter(|| exec.run_lifted(&my_fold, &graph, black_box(&root), &lift));
                    }
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_executors);
criterion_main!(benches);
