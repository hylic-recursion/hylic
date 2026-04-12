//! Explorative: Lift composition with nested GATs.
//!
//! Tests whether Rust compiles:
//! 1. IdentityLift — trivial LiftOps where everything passes through
//! 2. ComposedLift<L1, L2> — chain two lifts, nested GAT application
//! 3. A concrete composition: IdentityLift ∘ a type-changing lift
//!
//! This is the critical test for the lift-composition design.

use std::sync::Arc;
use crate::domain::shared::{self as dom, fold};
use crate::domain::shared::fold::Fold;
use crate::graph::{self, Treeish};
use crate::ops::LiftOps;

// ── IdentityLift ────────────────────────────────────

struct IdentityLift;

impl<N: Clone + 'static, R: Clone + 'static> LiftOps<N, R, N> for IdentityLift {
    type LiftedH<H: Clone + 'static> = H;
    type LiftedR<H: Clone + 'static> = R;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }
    fn lift_fold<H: Clone + 'static>(&self, f: Fold<N, H, R>) -> Fold<N, H, R> { f }
    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── A concrete type-changing lift: doubles the node ─

/// Wraps usize nodes as (usize, usize) — a pair of (original, doubled).
struct DoubleLift;

impl<R: Clone + 'static> LiftOps<usize, R, (usize, usize)> for DoubleLift {
    type LiftedH<H: Clone + 'static> = H;
    type LiftedR<H: Clone + 'static> = R;

    fn lift_treeish(&self, t: Treeish<usize>) -> Treeish<(usize, usize)> {
        t.treemap(
            |n: &usize| (*n, n * 2),
            |pair: &(usize, usize)| pair.0,
        )
    }

    fn lift_fold<H: Clone + 'static>(&self, f: Fold<usize, H, R>) -> Fold<(usize, usize), H, R> {
        f.contramap(|pair: &(usize, usize)| pair.0)
    }

    fn lift_root(&self, root: &usize) -> (usize, usize) {
        (*root, root * 2)
    }
}

// ── ComposedLift ────────────────────────────────────

struct ComposedLift<L1, L2, Nmid> {
    first: L1,
    second: L2,
    _phantom: std::marker::PhantomData<fn() -> Nmid>,
}

impl<L1, L2, Nmid> ComposedLift<L1, L2, Nmid> {
    fn new(first: L1, second: L2) -> Self {
        ComposedLift { first, second, _phantom: std::marker::PhantomData }
    }
}

/// Composed lift: L1 then L2.
///
/// L1: LiftOps<N, R, Nmid> — first transform
/// L2: LiftOps<Nmid, R, N2> — second transform (R stays the same
///     for lifts that are transparent in R, which is the common case)
///
/// Nested GATs: LiftedH<H> = L2::LiftedH<L1::LiftedH<H>>
impl<N, R, Nmid, N2, L1, L2> LiftOps<N, R, N2> for ComposedLift<L1, L2, Nmid>
where
    N: Clone + 'static,
    R: Clone + 'static,
    Nmid: Clone + 'static,
    N2: Clone + 'static,
    L1: LiftOps<N, R, Nmid>,
    L2: LiftOps<Nmid, R, N2>,
{
    type LiftedH<H: Clone + 'static> = L2::LiftedH<L1::LiftedH<H>>;
    type LiftedR<H: Clone + 'static> = L2::LiftedR<L1::LiftedH<H>>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2> {
        self.second.lift_treeish(self.first.lift_treeish(t))
    }

    fn lift_fold<H: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N2, Self::LiftedH<H>, Self::LiftedR<H>> {
        self.second.lift_fold(self.first.lift_fold(f))
    }

    fn lift_root(&self, root: &N) -> N2 {
        self.second.lift_root(&self.first.lift_root(root))
    }
}

// ── Tests ───────────────────────────────────────────

#[test]
fn identity_lift_preserves() {
    let ch: Vec<Vec<usize>> = vec![vec![1, 2], vec![3], vec![], vec![]];
    let treeish = graph::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &c in &ch[*n] { cb(&c); }
    });
    let f = fold::fold(
        |n: &usize| *n as u64,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let lift = IdentityLift;
    let lt = lift.lift_treeish(treeish.clone());
    let lf = lift.lift_fold(f.clone());

    let original = dom::FUSED.run(&f, &treeish, &0);
    let lifted = dom::FUSED.run(&lf, &lt, &0);
    assert_eq!(original, lifted);
}

#[test]
fn double_lift_changes_node_type() {
    let ch: Vec<Vec<usize>> = vec![vec![1, 2], vec![3], vec![], vec![]];
    let treeish = graph::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &c in &ch[*n] { cb(&c); }
    });
    let f = fold::fold(
        |n: &usize| *n as u64,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let lift = DoubleLift;
    let lt: Treeish<(usize, usize)> = lift.lift_treeish(treeish.clone());
    let lf: Fold<(usize, usize), u64, u64> = lift.lift_fold(f.clone());

    let original = dom::FUSED.run(&f, &treeish, &0);
    let lifted = dom::FUSED.run(&lf, &lt, &(0, 0));
    // contramap extracts .0, so same computation
    assert_eq!(original, lifted);
}

#[test]
fn composed_identity_then_double() {
    let ch: Vec<Vec<usize>> = vec![vec![1, 2], vec![3], vec![], vec![]];
    let treeish = graph::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &c in &ch[*n] { cb(&c); }
    });
    let f = fold::fold(
        |n: &usize| *n as u64,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    // Compose: IdentityLift then DoubleLift
    let composed = ComposedLift::<_, _, usize>::new(IdentityLift, DoubleLift);

    let lt: Treeish<(usize, usize)> = composed.lift_treeish(treeish.clone());
    let lf: Fold<(usize, usize), u64, u64> = composed.lift_fold(f.clone());

    let original = dom::FUSED.run(&f, &treeish, &0);
    let lifted = dom::FUSED.run(&lf, &lt, &composed.lift_root(&0));
    assert_eq!(original, lifted);
}

#[test]
fn composed_double_then_identity() {
    let ch: Vec<Vec<usize>> = vec![vec![1, 2], vec![3], vec![], vec![]];
    let treeish = graph::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &c in &ch[*n] { cb(&c); }
    });
    let f = fold::fold(
        |n: &usize| *n as u64,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    // Compose: DoubleLift then IdentityLift on the doubled type
    let composed = ComposedLift::<_, _, (usize, usize)>::new(DoubleLift, IdentityLift);

    let lt = composed.lift_treeish(treeish.clone());
    let lf = composed.lift_fold(f.clone());

    let original = dom::FUSED.run(&f, &treeish, &0);
    let lifted = dom::FUSED.run(&lf, &lt, &composed.lift_root(&0));
    assert_eq!(original, lifted);
}
