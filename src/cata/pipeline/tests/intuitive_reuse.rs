//! "Intuitive" cases the user should be able to write without
//! friction: reusing a pipeline across multiple runs; chaining two
//! user-written lifts; a custom lift that changes both N and R.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExec};
use crate::domain::Domain;
use crate::domain::shared::{self as dom, fold::fold};
use crate::graph::edgy_visit;
use crate::ops::Lift;

fn basic() -> SeedPipeline<u64, u64, u64, u64> {
    let ch: Arc<Vec<Vec<u64>>> = Arc::new(vec![
        vec![1, 2], vec![3], vec![], vec![],
        vec![1],    // node 4: one child (1)
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

#[test]
fn reuse_pipeline_across_runs() {
    // Same pipeline, two entry-seed sets, both succeed independently.
    let pipe = basic();

    let r1 = pipe.run_from_slice(&dom::FUSED, &[0u64], 0u64);
    // 0 + 1 + 2 + 3 = 6.
    assert_eq!(r1, 6);

    let r2 = pipe.run_from_slice(&dom::FUSED, &[4u64], 0u64);
    // 4 + 1 + 3 = 8. (ch[4] = [1]; ch[1] = [3]; ch[3] = [].)
    assert_eq!(r2, 8);

    // Original pipe is still usable — PipelineSource takes &self.
    let r3 = pipe.run_from_slice(&dom::FUSED, &[0u64, 4u64], 0u64);
    // 6 + 8 = 14.
    assert_eq!(r3, 14);
}

#[test]
fn two_user_lifts_in_series() {
    // Two distinct user-written lifts composed. Neither uses
    // library shape-lifts; each is declared in this test.

    #[derive(Clone, Copy)]
    struct AddToR(u64);   // Adds its constant to every R accumulate.

    use crate::domain::Shared;
    impl<N, H, R> Lift<Shared, N, H, R> for AddToR
    where N: Clone + 'static, H: Clone + 'static, R: Clone + Into<u64> + From<u64> + 'static,
    {
        type N2 = N; type MapH = H; type MapR = R;
        fn apply<Seed, T>(
            &self,
            grow:    <Shared as Domain<N>>::Grow<Seed, N>,
            treeish: <Shared as Domain<N>>::Graph<N>,
            fold_in: <Shared as Domain<N>>::Fold<H, R>,
            cont: impl FnOnce(
                <Shared as Domain<N>>::Grow<Seed, N>,
                <Shared as Domain<N>>::Graph<N>,
                <Shared as Domain<N>>::Fold<H, R>,
            ) -> T,
        ) -> T
        where Seed: Clone + 'static,
        {
            let addend = self.0;
            let wrapped = fold_in.wrap_finalize(move |h, orig| {
                let r: R = orig(h);
                let as_u: u64 = r.into();
                R::from(as_u + addend)
            });
            cont(grow, treeish, wrapped)
        }
    }

    #[derive(Clone, Copy)]
    struct MulByTwo;

    impl<N, H, R> Lift<Shared, N, H, R> for MulByTwo
    where N: Clone + 'static, H: Clone + 'static, R: Clone + Into<u64> + From<u64> + 'static,
    {
        type N2 = N; type MapH = H; type MapR = R;
        fn apply<Seed, T>(
            &self,
            grow:    <Shared as Domain<N>>::Grow<Seed, N>,
            treeish: <Shared as Domain<N>>::Graph<N>,
            fold_in: <Shared as Domain<N>>::Fold<H, R>,
            cont: impl FnOnce(
                <Shared as Domain<N>>::Grow<Seed, N>,
                <Shared as Domain<N>>::Graph<N>,
                <Shared as Domain<N>>::Fold<H, R>,
            ) -> T,
        ) -> T
        where Seed: Clone + 'static,
        {
            let wrapped = fold_in.wrap_finalize(move |h, orig| {
                let r: R = orig(h);
                R::from(r.into() * 2)
            });
            cont(grow, treeish, wrapped)
        }
    }

    // Chain: base → .lift() → AddToR(100) → MulByTwo
    // finalize of a subtree produces orig then +100 then *2.
    // Post-order on a tree of one node with result h:
    //   innermost finalize(h) = h.
    //   AddToR: h + 100.
    //   MulByTwo: (h + 100) * 2.
    // But fold.wrap_finalize composes OUTWARD from base — the LAST
    // applied wrap sees the OUTERMOST. So the chain above wraps in
    // this order:
    //   inner =            base.finalize      = h
    //   AddToR wraps:      base.finalize + 100
    //   MulByTwo wraps:    (base.finalize + 100) * 2
    // Called bottom-up at each node in the treeish.
    //
    // Tree: 0 → {1, 2}; 1 → {3}.
    // Each node's finalize is applied at its own level:
    //   node 3 (leaf): heap = 3.  acc of children = none.
    //     chain finalize: (3 + 100) * 2 = 206.
    //   node 1: heap = 1. acc(206) → 207.
    //     chain finalize: (207 + 100) * 2 = 614.
    //   node 2 (leaf): heap = 2.
    //     chain finalize: (2 + 100) * 2 = 204.
    //   node 0: heap = 0. acc(614) → 614. acc(204) → 818.
    //     chain finalize: (818 + 100) * 2 = 1836.
    //   Entry: heap = 0. acc(1836) → 1836.
    //     chain finalize: (1836 + 100) * 2 = 3872.
    let r = basic()
        .lift()
        .apply_pre_lift(AddToR(100))
        .apply_pre_lift(MulByTwo)
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);
    assert_eq!(r, 3872);
}

#[test]
fn lift_that_changes_both_n_and_r() {
    // Compose coalgebra N-change (Stage 1 contramap_node) with a
    // Stage-2 R-transform (map), producing a pipeline whose N and
    // R are both different from the base.

    #[derive(Clone, Debug, PartialEq)]
    struct N2(u64);

    let r: String = basic()
        .contramap_node(|n: &u64| N2(*n * 10), |w: &N2| w.0 / 10)
        .lift()
        .map(
            |r: &u64| format!("sum={r}"),
            |s: &String| s.strip_prefix("sum=").unwrap().parse::<u64>().unwrap(),
        )
        .run_from_slice(&dom::FUSED, &[0u64], 0u64);

    // After contramap_node, N is N2 at the traversal level; the
    // fold still operates on the N=u64 value (via contramap back).
    // Fold sums: 0 + 1 + 2 + 3 = 6.
    // Then map: "sum=6".
    assert_eq!(r, "sum=6".to_string());
}
