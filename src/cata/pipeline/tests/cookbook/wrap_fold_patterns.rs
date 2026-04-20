//! Cookbook: fold-side wrapping patterns.
//!
//! Demonstrates wrap_init, wrap_accumulate, wrap_finalize, zipmap
//! against a realistic "Task with cost_ms" scenario.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExecSeed};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::edgy_visit;
use crate::domain::Shared;

#[derive(Clone, Debug)]
struct Task {
    id:      u32,
    cost_ms: u32,
    depends: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
struct Cost {
    total_ms: u32,
    skipped:  u32,
}

fn task_registry() -> Arc<Vec<Task>> {
    Arc::new(vec![
        Task { id: 0, cost_ms: 50,  depends: vec![1, 2] },
        Task { id: 1, cost_ms: 20,  depends: vec![3]    },
        Task { id: 2, cost_ms: 30,  depends: vec![]     },
        Task { id: 3, cost_ms: 100, depends: vec![]     },
    ])
}

fn cost_pipeline() -> SeedPipeline<Shared, Task, u32, Cost, Cost> {
    let reg = task_registry();
    let reg_grow = reg.clone();
    let reg_seeds = reg.clone();

    let base_fold = fold(
        |n: &Task| Cost { total_ms: n.cost_ms, skipped: 0 },
        |h: &mut Cost, c: &Cost| { h.total_ms += c.total_ms; h.skipped += c.skipped; },
        |h: &Cost| h.clone(),
    );
    let seeds = edgy_visit(move |n: &Task, cb: &mut dyn FnMut(&u32)| {
        let _ = &reg_seeds;
        for d in &n.depends { cb(d); }
    });
    SeedPipeline::new(
        move |s: &u32| reg_grow[*s as usize].clone(),
        seeds,
        &base_fold,
    )
}

#[test]
fn wrap_init_traces_nodes_visited() {
    use std::sync::Mutex;
    let seen: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_for_closure = seen.clone();

    let r: Cost = cost_pipeline()
        .lift()
        .wrap_init(move |n: &Task, orig: &dyn Fn(&Task) -> Cost| {
            seen_for_closure.lock().unwrap().push(n.id);
            orig(n)
        })
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u32], Cost::default());

    assert_eq!(r.total_ms, 50 + 20 + 30 + 100);
    let mut ids = seen.lock().unwrap().clone();
    ids.sort();
    assert_eq!(ids, vec![0, 1, 2, 3]);
}

#[test]
fn wrap_accumulate_skips_expensive_children() {
    // wrap_accumulate is a uniform wrap — it applies at every fold
    // level including Entry. Here we skip any accumulated child whose
    // total exceeds 150 (so only the whole-tree sum at Entry gets
    // routed into `skipped`, while all intra-tree accumulation runs
    // normally).
    let r: Cost = cost_pipeline()
        .lift()
        .wrap_accumulate(|h: &mut Cost, c: &Cost, orig: &dyn Fn(&mut Cost, &Cost)| {
            if c.total_ms > 150 { h.skipped += c.total_ms; return; }
            orig(h, c);
        })
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u32], Cost::default());

    // Full tree sum = 200; > 150 → Entry routes it to skipped.
    assert_eq!(r.total_ms, 0);
    assert_eq!(r.skipped, 200);
}

#[test]
fn wrap_finalize_clamps_total() {
    let r: Cost = cost_pipeline()
        .lift()
        .wrap_finalize(|h: &Cost, orig: &dyn Fn(&Cost) -> Cost| {
            let mut out = orig(h);
            if out.total_ms > 80 { out.total_ms = 80; }
            out
        })
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u32], Cost::default());

    assert!(r.total_ms <= 80);
}

#[test]
fn zipmap_classifies_by_total() {
    let r: (Cost, &'static str) = cost_pipeline()
        .lift()
        .zipmap(|c: &Cost| -> &'static str {
            if c.total_ms < 50  { "cheap" }
            else if c.total_ms < 150 { "moderate" }
            else                      { "heavy" }
        })
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u32], Cost::default());

    assert_eq!(r.0.total_ms, 200);
    assert_eq!(r.1, "heavy");
}
