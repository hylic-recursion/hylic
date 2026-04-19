//! Stage-1 reshape primitive + the four coalgebra sugars.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExec};
use crate::domain::shared::{self as dom, fold::fold};
use crate::graph::edgy_visit;

fn flat_children(flat: Vec<Vec<u64>>) -> Arc<Vec<Vec<u64>>> { Arc::new(flat) }

fn basic_pipeline() -> SeedPipeline<u64, u64, u64, u64> {
    let ch = flat_children(vec![vec![1, 2], vec![3], vec![], vec![]]);
    let ch_for_seeds = ch.clone();
    let base_fold = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch_for_seeds.get(*n as usize) {
            for k in kids { cb(k); }
        }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[test]
fn filter_seeds_prunes() {
    // 0 → {1,2}; 1 → {3}; sum = 0 + 1 + 3 + 2 = 6.
    let r = basic_pipeline().run_from_slice(&dom::FUSED, &[0u64], 0u64);
    assert_eq!(r, 6);

    // filter out seed == 2: 0 → {1}; 1 → {3}; sum = 0 + 1 + 3 = 4.
    let r = basic_pipeline()
        .filter_seeds(|s: &u64| *s != 2)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    assert_eq!(r, 4);
}

#[test]
fn wrap_grow_intercepts_resolution() {
    // Multiply every grown node by 10. After grow'd, 10 and 20 are not
    // indices into `ch`, so they become leaves (seeds_from_node returns
    // empty). Tree shape: Entry → 0 → {10, 20}.
    let r = basic_pipeline()
        .wrap_grow(|s: &u64, orig: &dyn Fn(&u64) -> u64| orig(s) * 10)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // 10 → 10, 20 → 20; 0 → 0 + 10 + 20 = 30; Entry → 0 + 30 = 30.
    assert_eq!(r, 30);
}

#[test]
fn contramap_node_changes_n_type() {
    // Wrap N=u64 into a newtype tagged with a label.
    #[derive(Clone)]
    struct Tagged { v: u64 }

    let r = basic_pipeline()
        .contramap_node(
            |n: &u64| Tagged { v: *n },
            |t: &Tagged| t.v,
        )
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // Sum unchanged by the bijection.
    assert_eq!(r, 6);
}

#[test]
fn map_seed_changes_seed_type() {
    // Map u64 seeds to String seeds and back.
    let r = basic_pipeline()
        .map_seed(
            |s: &u64| format!("seed-{s}"),
            |s: &String| s.strip_prefix("seed-").unwrap().parse::<u64>().unwrap(),
        )
        .run_from_slice(&dom::FUSED, &["seed-0".to_string()], 0u64);
    assert_eq!(r, 6);
}

#[test]
fn reshape_is_fluent() {
    // Multiple Stage-1 transformations chain; each returns SeedPipeline.
    let r = basic_pipeline()
        .filter_seeds(|s: &u64| *s != 2)
        .wrap_grow(|s: &u64, orig: &dyn Fn(&u64) -> u64| orig(s) + 100)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // After filter: seeds_from_node(0) = [1] (was [1,2]).
    // wrap_grow: new_grow(s) = s + 100. Entry seed 0 → Node(100).
    // 100 is not an index in `ch`, so it's a leaf.
    // 100 → fin=100; Entry → heap=0, acc → 100.
    assert_eq!(r, 100);
}
