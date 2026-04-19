//! Power-user test: end-to-end pipeline with fluent chained sugars.
//!
//! Proves the CPS Lift design runs under FUSED with real types,
//! chaining filter_seeds + wrap_init + zipmap + apply_pre_lift(Explainer).

use std::sync::Arc;
use crate::cata::seed_lift::SeedPipeline;
use crate::cata::seed_lift::pipeline::exec::SeedPipelineExec;
use crate::domain::shared::{self as dom, fold::fold};
use crate::graph::edgy_visit;
use crate::prelude::{Explainer, ExplainerHeap, ExplainerResult};

#[test]
fn fluent_chain_with_explainer() {
    // Flat adjacency: 0 → {1, 2}; 1 → {3}; 2,3 leaves.
    let children: Arc<Vec<Vec<u64>>> = Arc::new(vec![
        vec![1, 2], vec![3], vec![], vec![],
    ]);

    let ch_for_seeds = children.clone();
    let base_fold = fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let seeds_fn = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch_for_seeds.get(*n as usize) {
            for k in kids { cb(k); }
        }
    });

    let pipeline = SeedPipeline::new(
        |s: &u64| *s,                        // grow
        seeds_fn,                             // seeds_from_node
        &base_fold,                           // fold
    )
    .filter_seeds(|s: &u64| *s != 2)         // shape-lift: filter
    .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)  // shape-lift: wrap_init
    .zipmap(|r: &u64| *r > 5)                // shape-lift: zipmap
    .apply_pre_lift(Explainer);              // user lift: Explainer

    let sentinel: u64 = 0;
    let entry_heap: ExplainerHeap<u64, u64, (u64, bool)> =
        ExplainerHeap::new(sentinel, 0u64);
    let result: ExplainerResult<u64, u64, (u64, bool)> =
        pipeline.run_from_slice(&dom::FUSED, &[0u64], entry_heap);

    // After filter_seeds(|s| *s != 2): 0 → {1}; 1 → {3}; 3 leaf.
    // wrap_init adds 1 to each node's init.
    // Per-subtree sums:
    //   3: init = 4, leaf                         → 4
    //   1: init = 2, child = [4]                  → 6
    //   0: init = 1, child = [6]                  → 7
    //   Entry: entry_heap = 0, child = [7]        → 7
    // zipmap appends (7, 7 > 5) = (7, true).
    assert_eq!(result.orig_result, (7, true));
    assert!(!result.heap.transitions.is_empty(), "trace populated");
}

#[test]
fn fluent_chain_parallel_funnel() {
    use crate::cata::exec::funnel;

    let children: Arc<Vec<Vec<u64>>> = Arc::new(vec![
        vec![1, 2], vec![3], vec![], vec![],
    ]);
    let ch_for_seeds = children.clone();
    let base_fold = fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let seeds_fn = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch_for_seeds.get(*n as usize) {
            for k in kids { cb(k); }
        }
    });

    let pipeline = SeedPipeline::new(|s: &u64| *s, seeds_fn, &base_fold)
        .filter_seeds(|s: &u64| *s != 2)
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .zipmap(|r: &u64| *r > 5)
        .apply_pre_lift(Explainer);

    let entry_heap: ExplainerHeap<u64, u64, (u64, bool)> =
        ExplainerHeap::new(0u64, 0u64);
    let result: ExplainerResult<u64, u64, (u64, bool)> = pipeline.run_from_slice(
        &dom::exec(funnel::Spec::default(4)),
        &[0u64],
        entry_heap,
    );

    assert_eq!(result.orig_result, (7, true));
    assert!(!result.heap.transitions.is_empty());
}
