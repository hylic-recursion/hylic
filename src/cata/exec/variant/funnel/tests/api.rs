//! User-facing API tests.
//!
//! These test the public API surface as a user would write it —
//! one-shot, session scope, explicit attach, axis transformation,
//! named presets, arena tuning.

use crate::domain::shared as dom;
use super::*;

// ── One-shot execution ──────────────────────────────

#[test]
fn one_shot_default() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(60, 4);
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let result = dom::exec(Spec::default(n_threads())).run(&fold, &graph, &tree);
    assert_eq!(result, expected);
}

// ── Session scope ───────────────────────────────────

#[test]
fn session_multi_run() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(60, 4);
    let expected = dom::FUSED.run(&fold, &graph, &tree);

    dom::exec(Spec::default(n_threads())).session(|s| {
        assert_eq!(s.run(&fold, &graph, &tree), expected);
        assert_eq!(s.run(&fold, &graph, &tree), expected);
    });
}

// ── Explicit attach ─────────────────────────────────

#[test]
fn explicit_attach() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(60, 4);
    let expected = dom::FUSED.run(&fold, &graph, &tree);

    Pool::with(n_threads(), |pool| {
        let exec = dom::exec(Spec::default(n_threads())).attach(pool);
        assert_eq!(exec.run(&fold, &graph, &tree), expected);
    });
}

#[test]
fn attach_multi_run() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 6);
    let expected = dom::FUSED.run(&fold, &graph, &tree);

    Pool::with(n_threads(), |pool| {
        let exec = dom::exec(Spec::default(n_threads())).attach(pool);
        for _ in 0..100 {
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        }
    });
}

// ── All named presets ───────────────────────────────

#[test]
fn all_presets_correct() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 8);
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();

    assert_eq!(dom::exec(Spec::default(nt)).run(&fold, &graph, &tree), expected, "Default");
    assert_eq!(dom::exec(Spec::for_graph_heavy(nt)).run(&fold, &graph, &tree), expected, "GraphHeavy");
    assert_eq!(dom::exec(Spec::for_wide_light(nt)).run(&fold, &graph, &tree), expected, "WideLight");
    assert_eq!(dom::exec(Spec::for_low_overhead(nt)).run(&fold, &graph, &tree), expected, "LowOverhead");
    assert_eq!(dom::exec(Spec::for_high_throughput(nt)).run(&fold, &graph, &tree), expected, "HighThroughput");
    assert_eq!(dom::exec(Spec::for_streaming_wide(nt)).run(&fold, &graph, &tree), expected, "StreamingWide");
    assert_eq!(dom::exec(Spec::for_deep_narrow(nt)).run(&fold, &graph, &tree), expected, "DeepNarrow");
}

#[test]
fn perworker_arrival_preset() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 6);
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();

    let spec = Spec::<policy::PerWorkerArrival>::new(
        nt,
        super::super::queue::per_worker::PerWorkerSpec { deque_capacity: 4096 },
        super::super::accumulate::on_arrival::OnArrivalSpec,
        super::super::wake::every_push::EveryPushSpec,
    );
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected);
}

#[test]
fn shared_default_preset() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 6);
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();

    let spec = Spec::<policy::SharedDefault>::new(
        nt,
        super::super::queue::shared::SharedSpec,
        super::super::accumulate::on_finalize::OnFinalizeSpec,
        super::super::wake::every_push::EveryPushSpec,
    );
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected);
}

// ── Axis transformation chains ──────────────────────

#[test]
fn transform_single_axis() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 8);
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();

    // Change queue only
    let spec = Spec::default(nt)
        .with_queue::<super::super::queue::Shared>(super::super::queue::shared::SharedSpec);
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected, "with_queue");

    // Change accumulate only
    let spec = Spec::default(nt)
        .with_accumulate::<super::super::accumulate::OnArrival>(super::super::accumulate::on_arrival::OnArrivalSpec);
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected, "with_accumulate");

    // Change wake only
    let spec = Spec::default(nt)
        .with_wake::<super::super::wake::OncePerBatch>(super::super::wake::once_per_batch::OncePerBatchSpec);
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected, "with_wake");
}

#[test]
fn transform_all_axes() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 8);
    let expected = dom::FUSED.run(&fold, &graph, &tree);

    let spec = Spec::default(n_threads())
        .with_queue::<super::super::queue::Shared>(super::super::queue::shared::SharedSpec)
        .with_accumulate::<super::super::accumulate::OnArrival>(super::super::accumulate::on_arrival::OnArrivalSpec)
        .with_wake::<super::super::wake::OncePerBatch>(super::super::wake::once_per_batch::OncePerBatchSpec);
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected);
}

// ── Preset == default + transforms ──────────────────

#[test]
fn preset_equals_transformation() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 20);
    let nt = n_threads();

    let expected = dom::FUSED.run(&fold, &graph, &tree);

    // WideLight preset
    let from_preset = dom::exec(Spec::for_wide_light(nt)).run(&fold, &graph, &tree);

    // Same axes via transformation
    let from_chain = dom::exec(
        Spec::default(nt)
            .with_queue::<super::super::queue::Shared>(super::super::queue::shared::SharedSpec)
            .with_accumulate::<super::super::accumulate::OnArrival>(super::super::accumulate::on_arrival::OnArrivalSpec)
    ).run(&fold, &graph, &tree);

    assert_eq!(from_preset, expected);
    assert_eq!(from_chain, expected);
    assert_eq!(from_preset, from_chain);
}

// ── Arena tuning ────────────────────────────────────

#[test]
fn arena_tuning() {
    let fold = sum_fold();
    let graph = n_graph();
    let tree = big_tree(200, 6);
    let expected = dom::FUSED.run(&fold, &graph, &tree);

    let spec = Spec::default(n_threads()).with_arena_capacity(8192, 16384);
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected);

    let spec = Spec::default(n_threads()).with_arena_capacity(512, 1024);
    assert_eq!(dom::exec(spec).run(&fold, &graph, &tree), expected);
}
