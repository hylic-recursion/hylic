//! Smoke tests for the `LiftedSugarsShared` blanket trait.
//!
//! Verifies that sugars written once on the trait are callable on
//! SeedPipeline, TreeishPipeline, and LiftedPipeline with identical
//! syntax (no `.lift()` ceremony required for Stage-1 types).

use std::sync::Arc;
use crate::cata::pipeline::{
    SeedPipeline, TreeishPipeline, PipelineExec, PipelineExecSeed,
    LiftedSugarsShared,
};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::domain::Shared;
use crate::graph::{edgy_visit, treeish};

fn seed_pipeline() -> SeedPipeline<Shared, u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![vec![1, 2], vec![3], vec![], vec![]]);
    let base_fold = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let seeds = edgy_visit(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
        if let Some(kids) = ch.get(*n as usize) { for k in kids { cb(k); } }
    });
    SeedPipeline::new(|s: &u64| *s, seeds, &base_fold)
}

#[test]
fn seed_pipeline_wrap_init_via_trait_no_lift_call() {
    // Note: no .lift() — trait auto-lifts.
    let r = seed_pipeline()
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    // wrap_init(+1) on tree {0→{1,2}, 1→{3}}:
    // 3:4; 1:2+4=6; 2:3; 0:1+6+3=10; Entry:0+10=10.
    assert_eq!(r, 10);
}

#[test]
fn seed_pipeline_chain_via_trait() {
    let r = seed_pipeline()
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .zipmap(|r: &u64| *r > 5)
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    assert_eq!(r, (10u64, true));
}

#[test]
fn treeish_pipeline_wrap_init_via_trait() {
    let t = treeish(|n: &u64| match *n {
        0 => vec![1, 2],
        1 => vec![3],
        _ => vec![],
    });
    let f = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let r: u64 = TreeishPipeline::<Shared, u64, u64, u64>::new(t, &f)
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &0u64);
    assert_eq!(r, 10);
}

#[test]
fn lifted_pipeline_also_supports_trait_methods() {
    // After .lift(), the LiftedPipeline impl of the trait kicks in.
    let r = seed_pipeline()
        .lift()
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    assert_eq!(r, 10);
}

#[test]
fn map_r_bi_changes_r_type_via_trait() {
    let r: String = seed_pipeline()
        .map_r_bi(
            |r: &u64| format!("sum={r}"),
            |s: &String| s.strip_prefix("sum=").unwrap().parse::<u64>().unwrap(),
        )
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    assert_eq!(r, "sum=6");
}
