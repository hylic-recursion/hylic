use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

use hylic::graph::treeish;
use hylic::fold;
use hylic::cata::Exec;
use hylic::prelude::uio_parallel;

fn busy_work(iterations: u64) -> u64 {
    let mut x: u64 = 0xDEAD_BEEF;
    for _ in 0..iterations {
        x = black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1));
    }
    x
}

fn spin_wait_us(micros: u64) {
    if micros == 0 { return; }
    let start = std::time::Instant::now();
    while start.elapsed().as_micros() < micros as u128 {
        std::hint::spin_loop();
    }
}

#[derive(Clone)]
struct Node { id: usize, children: Vec<Node> }

struct TreeSpec { node_count: usize, branch_factor: usize }

fn gen_tree(spec: &TreeSpec, seed: u64) -> Node {
    let mut rng = SimpleRng(seed);
    let mut built = 1;
    build_subtree(0, spec, &mut built, &mut rng)
}

fn build_subtree(id: usize, spec: &TreeSpec, built: &mut usize, rng: &mut SimpleRng) -> Node {
    if *built >= spec.node_count { return Node { id, children: vec![] }; }
    let max_ch = spec.branch_factor.min(spec.node_count - *built);
    let n_ch = if max_ch == 0 { 0 } else { 1 + rng.next_usize() % max_ch };
    let children = (0..n_ch).map(|_| {
        let cid = *built; *built += 1;
        build_subtree(cid, spec, built, rng)
    }).collect();
    Node { id, children }
}

struct SimpleRng(u64);
impl SimpleRng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0
    }
    fn next_usize(&mut self) -> usize { self.next_u64() as usize }
}

struct BenchConfig {
    name: &'static str,
    tree: TreeSpec,
    graph_latency_us: u64,
    graph_compute: u64,
    fold_compute: u64,
    asymmetric: bool,
}

fn configs() -> Vec<BenchConfig> {
    vec![
        // Minimal work — measures pure framework overhead
        BenchConfig {
            name: "overhead_only",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 0, fold_compute: 0,
            asymmetric: false,
        },
        // Light CPU work — typical fast resolution
        BenchConfig {
            name: "light_cpu",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 1_000, fold_compute: 1_000,
            asymmetric: false,
        },
        // I/O-dominant discovery (the real resolution case)
        BenchConfig {
            name: "io_discovery",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 100, graph_compute: 0, fold_compute: 500,
            asymmetric: false,
        },
        // Parse-heavy discovery
        BenchConfig {
            name: "parse_heavy",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 20_000, fold_compute: 500,
            asymmetric: false,
        },
        // Heavy fold (formatting, analysis)
        BenchConfig {
            name: "heavy_fold",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 500, fold_compute: 20_000,
            asymmetric: false,
        },
        // Balanced real-world
        BenchConfig {
            name: "balanced",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 50, graph_compute: 5_000, fold_compute: 5_000,
            asymmetric: false,
        },
        // Deep tree, I/O
        BenchConfig {
            name: "deep_io",
            tree: TreeSpec { node_count: 200, branch_factor: 2 },
            graph_latency_us: 100, graph_compute: 0, fold_compute: 500,
            asymmetric: false,
        },
        // Large tree, light work — stress allocation
        BenchConfig {
            name: "large_light",
            tree: TreeSpec { node_count: 1000, branch_factor: 10 },
            graph_latency_us: 0, graph_compute: 500, fold_compute: 500,
            asymmetric: false,
        },
    ]
}

/// Execution modes — the full 2×2 matrix:
///   traversal: fused (callback) vs rayon (par_iter)
///   fold: eager (direct) vs deferred (UIO lift, join_par)
enum Mode {
    Fused,           // sequential traversal, eager fold
    Rayon,           // parallel traversal, eager fold
    UioFused,        // sequential traversal, deferred fold (UIO)
    UioRayon,        // parallel traversal, deferred fold (UIO)
}

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

fn bench_executors(c: &mut Criterion) {
    let mut group = c.benchmark_group("executors");

    for cfg in configs() {
        let tree = gen_tree(&cfg.tree, 42);
        let gl = cfg.graph_latency_us;
        let gc = cfg.graph_compute;
        let fc = cfg.fold_compute;
        let asym = cfg.asymmetric;

        let graph = treeish(move |n: &Node| {
            spin_wait_us(gl);
            if gc > 0 {
                let work = if asym { gc * n.children.len() as u64 } else { gc };
                black_box(busy_work(work));
            }
            n.children.clone()
        });

        let my_fold = fold::simple_fold(
            move |n: &Node| {
                let work = if asym && fc > 0 { fc * (n.children.len() as u64 + 1) } else { fc };
                (n.id as u64).wrapping_add(busy_work(work))
            },
            |a: &mut u64, c: &u64| { *a = a.wrapping_add(*c); },
        );

        let modes = [Mode::Fused, Mode::Rayon, Mode::UioFused, Mode::UioRayon];

        for mode in &modes {
            group.bench_with_input(
                BenchmarkId::new(mode.name(), cfg.name),
                &(),
                |b, _| {
                    match mode {
                        Mode::Fused => {
                            let exec = Exec::fused();
                            b.iter(|| exec.run(&my_fold, &graph, black_box(&tree)));
                        }
                        Mode::Rayon => {
                            let exec = Exec::rayon();
                            b.iter(|| exec.run(&my_fold, &graph, black_box(&tree)));
                        }
                        Mode::UioFused => {
                            let exec = Exec::fused();
                            let lift = uio_parallel();
                            b.iter(|| exec.run_lifted(&my_fold, &graph, black_box(&tree), &lift));
                        }
                        Mode::UioRayon => {
                            let exec = Exec::fused();
                            let lift = uio_parallel().with_inner_exec(|| Exec::rayon());
                            b.iter(|| exec.run_lifted(&my_fold, &graph, black_box(&tree), &lift));
                        }
                    }
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_executors);
criterion_main!(benches);
