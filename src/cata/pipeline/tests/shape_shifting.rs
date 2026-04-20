//! Shape-shifting power-user tests — aggressive coalgebra+algebra
//! rewrites that intuitively "should just work" if the typestate
//! and transformations do their jobs.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExecSeed};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::edgy_visit;
use crate::domain::Shared;
use crate::prelude::{ExplainerHeap, ExplainerResult};

/// Flat adjacency: 0 → {1, 2}; 1 → {3}; 2, 3 leaves.
fn basic_pipeline() -> SeedPipeline<crate::domain::Shared, u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![
        vec![1, 2], vec![3], vec![], vec![],
    ]);
    let base_fold = fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch.get(*n as usize) { for k in kids { cb(k); } }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[derive(Clone, Debug, PartialEq)]
struct BoxedU64(u64);

#[test]
fn t1_stage1_heavy_reshape() {
    // Aggressive Stage-1 chain: change N → BoxedU64; change Seed →
    // String; filter seeds based on String; wrap grow. All four
    // coalgebra sugars compose; each type change is visible in the
    // resulting pipeline's type.
    let r = basic_pipeline()
        .contramap_node(
            |n: &u64| BoxedU64(*n),
            |b: &BoxedU64| b.0,
        )
        .map_seed(
            |s: &u64| format!("seed-{s}"),
            |s: &String| s.strip_prefix("seed-").unwrap().parse::<u64>().unwrap(),
        )
        .filter_seeds(|s: &String| s != "seed-2")
        .wrap_grow(|s: &String, orig: &dyn Fn(&String) -> BoxedU64| {
            // Wrap grow: add 1000 to the value on every grown node.
            let b = orig(s);
            BoxedU64(b.0 + 1000)
        })
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &["seed-0".to_string()], 0u64);

    // Tree after filter+wrap_grow:
    // entry seed "seed-0" → grow'd through +1000 → BoxedU64(1000).
    // Then seeds_from_node(1000) = ch.get(1000) = None → leaf.
    // Fold: init(1000) = 1000; Entry accumulate 1000 from entry_heap 0 = 1000.
    assert_eq!(r, 1000);
}

#[test]
fn t2_full_coalgebra_and_algebra_shape_shift() {
    // Stage 1 shape-shift, then lift, then every Stage-2 sugar,
    // ending with Explainer. Assert the ExplainerResult comes
    // through with the full shape-shifted types.
    let result: ExplainerResult<BoxedU64, u64, i128> = basic_pipeline()
        .contramap_node(
            |n: &u64| BoxedU64(*n),
            |b: &BoxedU64| b.0,
        )
        .filter_seeds(|s: &u64| *s != 2)
        .lift()
        .wrap_init(|n: &BoxedU64, orig: &dyn Fn(&BoxedU64) -> u64| orig(n) + 10)
        .zipmap(|r: &u64| *r > 100)
        .map(
            |r: &(u64, bool)| (r.0 as i128) + (if r.1 { 1000 } else { 0 }),
            |r: &i128| {
                let v = (*r % 1000) as u64;
                let flag = *r >= 1000;
                (v, flag)
            },
        )
        .apply_pre_lift(Shared::explainer_lift::<BoxedU64, u64, i128>())
        .run_from_slice(
            &dom::exec(funnel::Spec::default(4)),
            &[0u64],
            ExplainerHeap::new(BoxedU64(0), 0u64),
        );

    // Trace:
    // filter_seeds keeps 1 (drops 2); 0 → {1}; 1 → {3}.
    // contramap_node wraps to BoxedU64 but fold sees plain u64.
    // wrap_init(+10): each node's init = val + 10.
    // Per-subtree sums of (val + 10):
    //   3 → 13
    //   1 → 11 + 13 = 24
    //   0 → 10 + 24 = 34
    // Entry → 0 + 34 = 34.
    // zipmap → (34, false)  [34 > 100? no]
    // map → 34i128 + 0 (flag false) = 34.
    assert_eq!(result.orig_result, 34i128);
    assert!(!result.heap.transitions.is_empty());
}
