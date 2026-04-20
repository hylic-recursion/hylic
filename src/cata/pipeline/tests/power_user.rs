//! End-to-end power-user fluent chains under both Fused and Funnel.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExecSeed, LiftedSugarsShared};
use crate::domain::shared::{self as dom, fold::fold};
use crate::graph::edgy_visit;
use crate::domain::Shared;
use crate::prelude::{ExplainerHeap, ExplainerResult};

fn tree_pipeline() -> SeedPipeline<crate::domain::Shared, u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![vec![1, 2], vec![3], vec![], vec![]]);
    let base_fold = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch.get(*n as usize) {
            for k in kids { cb(k); }
        }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[test]
fn full_chain_with_explainer_fused() {
    let result: ExplainerResult<u64, u64, (u64, bool)> = tree_pipeline()
        .filter_seeds(|s: &u64| *s != 2)                                   // Stage 1
        .lift()                                                             // ─ transition
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)      // Stage 2
        .zipmap(|r: &u64| *r > 5)
        .then_lift(Shared::explainer_lift::<u64, u64, (u64, bool)>())
        .run_from_slice(
            &dom::FUSED,
            &[0u64],
            ExplainerHeap::new(0u64, 0u64),
        );
    // After filter_seeds(|s| *s != 2): 0 → {1}; 1 → {3}; 3 leaf.
    // wrap_init adds 1 to each node's init.
    // Per-subtree: 3 → 4; 1 → 2 + 4 = 6; 0 → 1 + 6 = 7. Entry → 7.
    // zipmap → (7, true).
    assert_eq!(result.orig_result, (7u64, true));
    assert!(!result.heap.transitions.is_empty(), "trace populated");
}

#[test]
fn full_chain_with_explainer_funnel() {
    use crate::cata::exec::funnel;

    let result: ExplainerResult<u64, u64, (u64, bool)> = tree_pipeline()
        .filter_seeds(|s: &u64| *s != 2)
        .lift()
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .zipmap(|r: &u64| *r > 5)
        .then_lift(Shared::explainer_lift::<u64, u64, (u64, bool)>())
        .run_from_slice(
            &dom::exec(funnel::Spec::default(4)),
            &[0u64],
            ExplainerHeap::new(0u64, 0u64),
        );
    assert_eq!(result.orig_result, (7u64, true));
    assert!(!result.heap.transitions.is_empty());
}

#[test]
fn stage1_and_stage2_compose_meaningfully() {
    // Chain everything interesting, both stages.
    let result = tree_pipeline()
        .filter_seeds(|s: &u64| *s != 2)       // Stage 1: prune
        .wrap_grow(|s: &u64, o: &dyn Fn(&u64) -> u64| o(s) * 2)   // Stage 1: double on grow
        .lift()
        .wrap_init(|n: &u64, o: &dyn Fn(&u64) -> u64| o(n) + 100) // Stage 2: +100 at init
        .map_r_bi(
            |r: &u64| *r as i64,                // Stage 2: R bijection to i64
            |r: &i64| *r as u64,
        )
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // After filter: seeds_from_node(0)=[1]; (2) excluded. wrap_grow: new_grow(s)=s*2.
    // Entry seed 0 → Node(0). 0's seeds_from_node(0)=[1] → map(new_grow)=[2].
    // Node(2): seeds_from_node(2)=[] — leaf.
    // wrap_init: init(n) = n + 100. So init(2)=102, init(0)=100.
    // Fold: 2→102; 0→100+[102]=202; Entry→0+[202]=202.
    // map: u64 → i64. r = 202.
    assert_eq!(result, 202i64);
}
