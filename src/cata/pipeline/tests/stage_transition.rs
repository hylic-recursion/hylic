//! The .lift() transition from Stage 1 to Stage 2.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, LiftedPipeline, PipelineExecSeed};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::edgy_visit;
use crate::ops::IdentityLift;

fn basic_pipeline() -> SeedPipeline<crate::domain::Shared, u64, u64, u64, u64> {
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
fn lift_produces_identity_lifted_pipeline() {
    let p: LiftedPipeline<SeedPipeline<crate::domain::Shared, u64, u64, u64, u64>, IdentityLift> = basic_pipeline().lift();
    let r = p.run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    assert_eq!(r, 6);
}

#[test]
fn lift_preserves_semantics_of_base_run() {
    // A SeedPipeline and its .lift() run should produce the same R.
    let direct = basic_pipeline().run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    let lifted = basic_pipeline().lift().run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    assert_eq!(direct, lifted);
}

#[test]
fn coalgebra_must_precede_algebra() {
    // The Phase-3 typestate enforces ordering via method presence.
    // Compile-time check: `lifted.filter_seeds(...)` would not compile
    // because LiftedPipeline has no filter_seeds method.
    //
    // The runtime test here just exercises the legal order.
    let r = basic_pipeline()
        .filter_seeds(|s: &u64| *s != 2)  // Stage 1
        .lift()                           // transition
        .zipmap(|r: &u64| *r > 0)         // Stage 2
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    // After filter: 0→{1}; 1→{3}. Sum = 0 + 1 + 3 = 4. zipmap → (4, true).
    assert_eq!(r, (4u64, true));
}

#[test]
fn clone_and_branch_before_lifting() {
    // Stage-1 pipelines are Clone — users can branch into multiple
    // different lift chains.
    let base = basic_pipeline();

    let unlifted = base.clone().run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    let zipped = base.clone().lift().zipmap(|r: &u64| *r).run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);
    let wrapped = base.lift().wrap_init(|n: &u64, o: &dyn Fn(&u64) -> u64| o(n) + 1)
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &[0u64], 0u64);

    assert_eq!(unlifted, 6);
    assert_eq!(zipped, (6, 6));
    assert_eq!(wrapped, 10);
}
