use std::hint::black_box;
use std::sync::Arc;
use hylic::graph::{treeish, Treeish};
use hylic::fold::{self, Fold};
use hylic::cata::Exec;
use hylic::prelude::{ParLazy, ParEager, WorkPool};

// ── Tree generation ─────────────────────────────────────────

pub type NodeId = usize;

pub struct TreeSpec {
    pub node_count: usize,
    pub branch_factor: usize,
}

pub fn gen_tree(spec: &TreeSpec) -> (Arc<Vec<Vec<NodeId>>>, usize) {
    let mut children: Vec<Vec<NodeId>> = Vec::new();
    children.push(vec![]);
    let mut next_id = 1usize;
    let mut level_start = 0;
    let mut level_end = 1;

    while next_id < spec.node_count {
        let mut new_level_end = level_end;
        for parent in level_start..level_end {
            let n_ch = spec.branch_factor.min(spec.node_count - next_id);
            if n_ch == 0 { break; }
            let mut my_children = Vec::with_capacity(n_ch);
            for _ in 0..n_ch {
                if next_id >= spec.node_count { break; }
                children.push(vec![]);
                my_children.push(next_id);
                next_id += 1;
                new_level_end += 1;
            }
            children[parent] = my_children;
        }
        level_start = level_end;
        level_end = new_level_end;
        if level_start == level_end { break; }
    }

    let count = children.len();
    (Arc::new(children), count)
}

// ── Work simulation ─────────────────────────────────────────

pub fn busy_work(iterations: u64) -> u64 {
    let mut x: u64 = 0xDEAD_BEEF;
    for _ in 0..iterations {
        x = black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1));
    }
    x
}

pub fn spin_wait_us(micros: u64) {
    if micros == 0 { return; }
    let start = std::time::Instant::now();
    while start.elapsed().as_micros() < micros as u128 {
        std::hint::spin_loop();
    }
}

// ── Workload config ─────────────────────────────────────────

pub struct WorkloadConfig {
    pub name: &'static str,
    pub tree: TreeSpec,
    pub graph_latency_us: u64,
    pub graph_compute: u64,
    pub fold_compute: u64,
}

pub fn all_configs() -> Vec<WorkloadConfig> {
    vec![
        WorkloadConfig {
            name: "0us:overhead",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 0, fold_compute: 0,
        },
        WorkloadConfig {
            name: "10us:light",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 5_000, fold_compute: 5_000,
        },
        WorkloadConfig {
            name: "100us:graph-heavy",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 100_000, fold_compute: 5_000,
        },
        WorkloadConfig {
            name: "100us:fold-heavy",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 5_000, fold_compute: 100_000,
        },
        WorkloadConfig {
            name: "200us:io",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 200, graph_compute: 0, fold_compute: 5_000,
        },
        WorkloadConfig {
            name: "200us:balanced",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 100_000, fold_compute: 100_000,
        },
        WorkloadConfig {
            name: "1ms:heavy",
            tree: TreeSpec { node_count: 200, branch_factor: 8 },
            graph_latency_us: 0, graph_compute: 500_000, fold_compute: 500_000,
        },
        WorkloadConfig {
            name: "200us:deep",
            tree: TreeSpec { node_count: 200, branch_factor: 2 },
            graph_latency_us: 200, graph_compute: 0, fold_compute: 5_000,
        },
        WorkloadConfig {
            name: "100us:large500",
            tree: TreeSpec { node_count: 500, branch_factor: 10 },
            graph_latency_us: 0, graph_compute: 50_000, fold_compute: 50_000,
        },
    ]
}

// ── Build graph + fold from config ──────────────────────────

pub fn prepare(cfg: &WorkloadConfig) -> (Treeish<NodeId>, Fold<NodeId, u64, u64>, NodeId) {
    let (ch, _count) = gen_tree(&cfg.tree);
    let gl = cfg.graph_latency_us;
    let gc = cfg.graph_compute;
    let fc = cfg.fold_compute;
    let graph = treeish(move |n: &NodeId| {
        spin_wait_us(gl);
        if gc > 0 { black_box(busy_work(gc)); }
        ch[*n].clone()
    });
    let my_fold = fold::simple_fold(
        move |_n: &NodeId| { if fc > 0 { busy_work(fc) } else { 0u64 } },
        |a: &mut u64, c: &u64| { *a = a.wrapping_add(*c); },
    );
    (graph, my_fold, 0)
}

// ── Execution modes ─────────────────────────────────────────

pub const MODE_NAMES: [&str; 6] = [
    "fused", "rayon", "parref+fused", "parref+rayon", "eager+fused", "eager+rayon",
];

pub fn run_mode(
    name: &str,
    fold: &Fold<NodeId, u64, u64>,
    graph: &Treeish<NodeId>,
    root: &NodeId,
    pool: &Arc<WorkPool>,
) -> u64 {
    match name {
        "fused"        => Exec::fused().run(fold, graph, root),
        "rayon"        => Exec::rayon().run(fold, graph, root),
        "parref+fused" => Exec::fused().run_lifted(&ParLazy::lift(), fold, graph, root),
        "parref+rayon" => Exec::rayon().run_lifted(&ParLazy::lift(), fold, graph, root),
        "eager+fused"  => Exec::fused().run_lifted(&ParEager::lift(pool), fold, graph, root),
        "eager+rayon"  => Exec::rayon().run_lifted(&ParEager::lift(pool), fold, graph, root),
        _ => panic!("unknown mode: {name}"),
    }
}
