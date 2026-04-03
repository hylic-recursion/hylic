use hylic::domain::shared as dom;
// Executor — trait, needed for .run()
use hylic::prelude::{ParLazy, ParEager, EagerSpec, WorkPool, WorkPoolSpec};
use std::sync::Arc;
use std::hint::black_box;
use std::time::Instant;

type NodeId = usize;
const ROOT: NodeId = 0;

fn gen_tree(node_count: usize, branch_factor: usize) -> (Arc<Vec<Vec<NodeId>>>, usize) {
    let mut children: Vec<Vec<NodeId>> = vec![vec![]];
    let mut next_id = 1usize;
    let mut level_start = 0;
    let mut level_end = 1;
    while next_id < node_count {
        let mut new_end = level_end;
        for parent in level_start..level_end {
            let n_ch = branch_factor.min(node_count - next_id);
            if n_ch == 0 { break; }
            let mut my_ch = Vec::with_capacity(n_ch);
            for _ in 0..n_ch {
                if next_id >= node_count { break; }
                children.push(vec![]);
                my_ch.push(next_id);
                next_id += 1;
                new_end += 1;
            }
            children[parent] = my_ch;
        }
        level_start = level_end;
        level_end = new_end;
        if level_start == level_end { break; }
    }
    let count = children.len();
    (Arc::new(children), count)
}

fn busy_work(iterations: u64) -> u64 {
    let mut x: u64 = 0xDEAD_BEEF;
    for _ in 0..iterations { x = black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1)); }
    x
}

fn timed(name: &str, iters: u32, expected: u64, f: impl Fn() -> u64) {
    let t = Instant::now();
    let mut result = 0u64;
    for _ in 0..iters { result = f(); }
    let us = t.elapsed().as_micros() as f64 / iters as f64;
    eprintln!("  {:15}: {:>10.0}µs  result={}", name, us, result);
    assert_eq!(result, expected, "{} wrong result", name);
}

fn run_case(label: &str, nodes: usize, bf: usize, gc: u64, fc: u64, iters: u32) {
    let (ch, count) = gen_tree(nodes, bf);
    let graph = dom::treeish(move |n: &NodeId| { if gc > 0 { black_box(busy_work(gc)); } ch[*n].clone() });
    let init = move |_: &NodeId| { if fc > 0 { busy_work(fc) } else { 0u64 } };
    let acc = |a: &mut u64, c: &u64| { *a = a.wrapping_add(*c); };
    let fold = dom::simple_fold(init, acc);
    let expected = dom::FUSED.run(&fold, &graph, &ROOT);

    eprintln!("\n=== {} ({} nodes, bf={}) ===", label, count, bf);
    timed("fused",        iters, expected, || dom::FUSED.run(&fold, &graph, &ROOT));
    timed("rayon",        iters, expected, || dom::RAYON.run(&fold, &graph, &ROOT));
    WorkPool::with(WorkPoolSpec::threads(3), |pool| {
        timed("parref+fused", iters, expected, || dom::FUSED.run_lifted(&ParLazy::lift(pool), &fold, &graph, &ROOT));
        timed("parref+rayon", iters, expected, || dom::RAYON.run_lifted(&ParLazy::lift(pool), &fold, &graph, &ROOT));
        timed("eager+fused", iters, expected, || dom::FUSED.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &fold, &graph, &ROOT));
        timed("eager+rayon", iters, expected, || dom::RAYON.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &fold, &graph, &ROOT));
    });
}

#[test]
fn eager_overhead_branching() {
    run_case("0us:overhead bf=8", 200, 8, 0, 0, 20);
}

#[test]
fn eager_overhead_linear() {
    run_case("0us:overhead bf=1", 200, 1, 0, 0, 20);
}

#[test]
fn eager_light_branching() {
    run_case("10us:light bf=8", 200, 8, 5_000, 5_000, 5);
}

#[test]
fn eager_heavy_branching() {
    run_case("100us:balanced bf=8", 200, 8, 100_000, 100_000, 3);
}
