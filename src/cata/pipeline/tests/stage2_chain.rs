//! Stage-2 apply_pre_lift + the five algebra sugars.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExec};
use crate::domain::shared::{self as dom, fold::fold};
use crate::graph::edgy_visit;

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
fn wrap_init_adds_constant() {
    let r = basic_pipeline()
        .lift()
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // wrap_init adds 1 to each Node's init; Entry uses entry_heap directly.
    // 3 → 4; 2 → 3; 1 → 2 + [4] = 6; 0 → 1 + [6, 3] = 10; Entry → 0 + [10] = 10.
    assert_eq!(r, 10);
}

#[test]
fn zipmap_pairs_result() {
    let r = basic_pipeline()
        .lift()
        .zipmap(|r: &u64| *r > 5)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // base sum = 6; zipmap appends (6, true).
    assert_eq!(r, (6u64, true));
}

#[test]
fn map_bijectively_transforms_r() {
    let r: String = basic_pipeline()
        .lift()
        .map(
            |r: &u64| format!("sum={r}"),
            |s: &String| s.strip_prefix("sum=").unwrap().parse::<u64>().unwrap(),
        )
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    assert_eq!(r, "sum=6");
}

#[test]
fn fluent_chain_stacks_lifts() {
    let r = basic_pipeline()
        .lift()
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .zipmap(|r: &u64| *r > 10)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // base with wrap_init(+1): 10 (see wrap_init_adds_constant).
    // zipmap appends (10, 10 > 10) = (10, false).
    assert_eq!(r, (10u64, false));
}

#[test]
fn apply_pre_lift_accepts_user_lift() {
    // Prove users can write their own Lift impl and plug it in.
    use crate::domain::Domain;
    use crate::ops::Lift;

    #[derive(Clone, Copy)]
    struct NoOp;

    impl<D, N, H, R> Lift<D, N, H, R> for NoOp
    where D: Domain<N>,
          N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        type N2 = N; type MapH = H; type MapR = R;

        fn apply<Seed, T>(
            &self,
            grow:    <D as Domain<N>>::Grow<Seed, N>,
            treeish: <D as Domain<N>>::Graph<N>,
            fold:    <D as Domain<N>>::Fold<H, R>,
            cont: impl FnOnce(
                <D as Domain<N>>::Grow<Seed, N>,
                <D as Domain<N>>::Graph<N>,
                <D as Domain<N>>::Fold<H, R>,
            ) -> T,
        ) -> T
        where Seed: Clone + 'static,
        {
            cont(grow, treeish, fold)
        }
    }

    let r = basic_pipeline()
        .lift()
        .apply_pre_lift(NoOp)
        .apply_pre_lift(NoOp)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    assert_eq!(r, 6);
}
