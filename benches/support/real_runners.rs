//! "Real-life" baselines — written as a student would, no framework,
//! no init/accumulate/finalize pattern. Just recursive functions with
//! for-loops.
//!
//! These do the same total work as the hylic and hand-* runners
//! (same busy_work amounts), but structured as natural Rust code:
//! one function, inline logic, no abstractions.

use std::sync::Arc;
use super::tree::NodeId;
use super::work::{busy_work, spin_wait_us, WorkSpec};
use super::scenario::PreparedScenario;

/// Sequential: plain recursive function.
/// A grad student's first implementation.
pub fn real_seq(s: &PreparedScenario) -> u64 {
    let children = &s.children;
    let w = &s.work;

    fn solve(
        children: &[Vec<NodeId>],
        node: NodeId,
        init_work: u64, acc_work: u64, fin_work: u64,
        graph_work: u64, graph_io_us: u64,
    ) -> u64 {
        // "discover children" — the graph traversal cost
        spin_wait_us(graph_io_us);
        if graph_work > 0 { std::hint::black_box(busy_work(graph_work)); }

        // "process this node" — the init cost
        let mut result = if init_work > 0 { busy_work(init_work) } else { 0u64 };

        // recurse into children, accumulate
        for &child in &children[node] {
            let child_result = solve(children, child,
                init_work, acc_work, fin_work, graph_work, graph_io_us);
            if acc_work > 0 {
                result = result.wrapping_add(busy_work(acc_work));
            }
            result = result.wrapping_add(child_result);
        }

        // "finalize" — post-processing cost
        if fin_work > 0 {
            result = result.wrapping_add(busy_work(fin_work));
        }
        result
    }

    solve(children, s.root,
        w.init_work, w.accumulate_work, w.finalize_work,
        w.graph_work, w.graph_io_us)
}

/// Parallel: rayon par_iter on children, natural style.
/// What you'd write after reading the rayon docs for 5 minutes.
pub fn real_rayon(s: &PreparedScenario) -> u64 {
    use rayon::prelude::*;
    let children = &s.children;
    let w = &s.work;

    fn solve(
        children: &Arc<Vec<Vec<NodeId>>>,
        node: NodeId,
        init_work: u64, acc_work: u64, fin_work: u64,
        graph_work: u64, graph_io_us: u64,
    ) -> u64 {
        spin_wait_us(graph_io_us);
        if graph_work > 0 { std::hint::black_box(busy_work(graph_work)); }

        let mut result = if init_work > 0 { busy_work(init_work) } else { 0u64 };

        let ch = &children[node];
        if ch.len() <= 1 {
            for &child in ch {
                let r = solve(children, child,
                    init_work, acc_work, fin_work, graph_work, graph_io_us);
                if acc_work > 0 { result = result.wrapping_add(busy_work(acc_work)); }
                result = result.wrapping_add(r);
            }
        } else {
            let results: Vec<u64> = ch.par_iter()
                .map(|&c| solve(children, c,
                    init_work, acc_work, fin_work, graph_work, graph_io_us))
                .collect();
            for r in results {
                if acc_work > 0 { result = result.wrapping_add(busy_work(acc_work)); }
                result = result.wrapping_add(r);
            }
        }

        if fin_work > 0 {
            result = result.wrapping_add(busy_work(fin_work));
        }
        result
    }

    solve(children, s.root,
        w.init_work, w.accumulate_work, w.finalize_work,
        w.graph_work, w.graph_io_us)
}

pub const REAL_MODES: [&str; 2] = ["real-seq", "real-rayon"];

pub fn run_real(name: &str, s: &PreparedScenario) -> u64 {
    match name {
        "real-seq"   => real_seq(s),
        "real-rayon" => real_rayon(s),
        _ => panic!("unknown real mode: {name}"),
    }
}
