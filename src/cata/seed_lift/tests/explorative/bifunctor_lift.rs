//! Explorative: Lift as bifunctor on (H, R).
//!
//! R moves from trait-level to method-level. Both GATs take (H, R).
//! This enables a blanket ComposedLift impl — no OuterLift needed.
//!
//! The key question: does Rust compile
//!   L2::MapH<L1::MapH<H, R>, L1::MapR<H, R>>
//! — nested 2-parameter GAT application?
//!
//! Self-contained. No dependency on hylic's types.

use std::marker::PhantomData;
use std::sync::Arc;

// ── Minimal types ───────────────────────────────────

struct Fold<N, H, R> {
    init: Arc<dyn Fn(&N) -> H>,
    acc: Arc<dyn Fn(&mut H, &R)>,
    fin: Arc<dyn Fn(&H) -> R>,
}

impl<N, H, R> Clone for Fold<N, H, R> {
    fn clone(&self) -> Self {
        Fold { init: self.init.clone(), acc: self.acc.clone(), fin: self.fin.clone() }
    }
}

impl<N: 'static, H: 'static, R: 'static> Fold<N, H, R> {
    fn new(
        init: impl Fn(&N) -> H + 'static,
        acc: impl Fn(&mut H, &R) + 'static,
        fin: impl Fn(&H) -> R + 'static,
    ) -> Self {
        Fold { init: Arc::new(init), acc: Arc::new(acc), fin: Arc::new(fin) }
    }

    fn run(&self, node: &N, children: &[R]) -> R {
        let mut h = (self.init)(node);
        for c in children { (self.acc)(&mut h, c); }
        (self.fin)(&h)
    }

    fn contramap<NewN: 'static>(self, f: impl Fn(&NewN) -> N + 'static) -> Fold<NewN, H, R> {
        let init = self.init; let acc = self.acc; let fin = self.fin;
        Fold::new(move |n: &NewN| (init)(&f(n)), move |h, r| (acc)(h, r), move |h| (fin)(h))
    }
}

struct Treeish<N> {
    visit: Arc<dyn Fn(&N, &mut dyn FnMut(&N))>,
}

impl<N> Clone for Treeish<N> {
    fn clone(&self) -> Self { Treeish { visit: self.visit.clone() } }
}

impl<N: 'static> Treeish<N> {
    fn new(f: impl Fn(&N, &mut dyn FnMut(&N)) + 'static) -> Self {
        Treeish { visit: Arc::new(f) }
    }
    fn collect(&self, node: &N) -> Vec<N> where N: Clone {
        let mut v = Vec::new();
        (self.visit)(node, &mut |c| v.push(c.clone()));
        v
    }
    fn treemap<N2: Clone + 'static>(
        &self, co: impl Fn(&N) -> N2 + 'static, contra: impl Fn(&N2) -> N + 'static,
    ) -> Treeish<N2> {
        let inner = self.visit.clone();
        Treeish::new(move |n: &N2, cb: &mut dyn FnMut(&N2)| {
            inner(&contra(n), &mut |child: &N| { let mapped = co(child); cb(&mapped); });
        })
    }
}

fn recurse<N: Clone + 'static, H: 'static, R: 'static>(
    fold: &Fold<N, H, R>, treeish: &Treeish<N>, node: &N,
) -> R {
    let children: Vec<R> = treeish.collect(node).iter()
        .map(|c| recurse(fold, treeish, c)).collect();
    fold.run(node, &children)
}

// ── The bifunctor Lift trait ────────────────────────

trait Lift<N: 'static, N2: 'static> {
    type MapH<H: Clone + 'static, R: Clone + 'static>: Clone + 'static;
    type MapR<H: Clone + 'static, R: Clone + 'static>: Clone + 'static;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2>;
    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N2, Self::MapH<H, R>, Self::MapR<H, R>>;
    fn lift_root(&self, root: &N) -> N2;
}

// ── ComposedLift: blanket impl, no OuterLift ────────

struct ComposedLift<L1, L2, Nmid> {
    first: L1,
    second: L2,
    _mid: PhantomData<fn() -> Nmid>,
}

impl<L1, L2, Nmid> ComposedLift<L1, L2, Nmid> {
    fn compose(first: L1, second: L2) -> Self {
        ComposedLift { first, second, _mid: PhantomData }
    }
}

impl<N, Nmid, N2, L1, L2> Lift<N, N2> for ComposedLift<L1, L2, Nmid>
where
    N: Clone + 'static,
    Nmid: Clone + 'static,
    N2: Clone + 'static,
    L1: Lift<N, Nmid>,
    L2: Lift<Nmid, N2>,
{
    type MapH<H: Clone + 'static, R: Clone + 'static> =
        L2::MapH<L1::MapH<H, R>, L1::MapR<H, R>>;
    type MapR<H: Clone + 'static, R: Clone + 'static> =
        L2::MapR<L1::MapH<H, R>, L1::MapR<H, R>>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2> {
        self.second.lift_treeish(self.first.lift_treeish(t))
    }

    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N2, Self::MapH<H, R>, Self::MapR<H, R>> {
        self.second.lift_fold(self.first.lift_fold(f))
    }

    fn lift_root(&self, root: &N) -> N2 {
        self.second.lift_root(&self.first.lift_root(root))
    }
}

// ── Concrete lifts ──────────────────────────────────

/// Identity: pass everything through.
struct Id;

impl<N: Clone + 'static> Lift<N, N> for Id {
    type MapH<H: Clone + 'static, R: Clone + 'static> = H;
    type MapR<H: Clone + 'static, R: Clone + 'static> = R;
    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }
    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(&self, f: Fold<N, H, R>) -> Fold<N, H, R> { f }
    fn lift_root(&self, root: &N) -> N { root.clone() }
}

/// Tag: wraps i32 as (i32, &str). N-changing, H/R transparent.
struct Tag(&'static str);

impl Lift<i32, (i32, &'static str)> for Tag {
    type MapH<H: Clone + 'static, R: Clone + 'static> = H;
    type MapR<H: Clone + 'static, R: Clone + 'static> = R;
    fn lift_treeish(&self, t: Treeish<i32>) -> Treeish<(i32, &'static str)> {
        let tag = self.0;
        t.treemap(move |n: &i32| (*n, tag), |p: &(i32, &str)| p.0)
    }
    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: Fold<i32, H, R>,
    ) -> Fold<(i32, &'static str), H, R> {
        f.contramap(|p: &(i32, &'static str)| p.0)
    }
    fn lift_root(&self, root: &i32) -> (i32, &'static str) { (*root, self.0) }
}

/// Trace: wraps H with trace data (mini-Explainer). H AND R change.
#[derive(Clone)]
struct TraceHeap<H: Clone, R: Clone> {
    working: H,
    steps: Vec<R>,
}

#[derive(Clone)]
struct TraceResult<H: Clone, R: Clone> {
    orig: R,
    heap_snapshot: H,
    n_children: usize,
}

struct TraceLift;

impl<N: Clone + 'static> Lift<N, N> for TraceLift {
    type MapH<H: Clone + 'static, R: Clone + 'static> = TraceHeap<H, R>;
    type MapR<H: Clone + 'static, R: Clone + 'static> = TraceResult<H, R>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }

    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N, TraceHeap<H, R>, TraceResult<H, R>> {
        let f1 = f.clone(); let f2 = f.clone(); let f3 = f;
        Fold::new(
            move |n: &N| TraceHeap { working: (f1.init)(n), steps: Vec::new() },
            move |heap: &mut TraceHeap<H, R>, result: &TraceResult<H, R>| {
                (f2.acc)(&mut heap.working, &result.orig);
                heap.steps.push(result.orig.clone());
            },
            move |heap: &TraceHeap<H, R>| TraceResult {
                orig: (f3.fin)(&heap.working),
                heap_snapshot: heap.working.clone(),
                n_children: heap.steps.len(),
            },
        )
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── Test fixtures ───────────────────────────────────

fn test_tree() -> Treeish<i32> {
    let ch = vec![vec![1i32, 2], vec![3], vec![], vec![]];
    Treeish::new(move |n: &i32, cb: &mut dyn FnMut(&i32)| {
        if let Some(children) = ch.get(*n as usize) {
            for &c in children { cb(&c); }
        }
    })
}

fn sum_fold() -> Fold<i32, i64, i64> {
    Fold::new(|n: &i32| *n as i64, |h: &mut i64, c: &i64| *h += c, |h: &i64| *h)
}

// ── Tests ───────────────────────────────────────────

#[test]
fn baseline() {
    assert_eq!(recurse(&sum_fold(), &test_tree(), &0), 6);
}

#[test]
fn identity_lift() {
    let lift = Id;
    let lt = lift.lift_treeish(test_tree());
    let lf = lift.lift_fold(sum_fold());
    assert_eq!(recurse(&lf, &lt, &0), 6);
}

#[test]
fn tag_lift() {
    let lift = Tag("x");
    let lt = lift.lift_treeish(test_tree());
    let lf = lift.lift_fold(sum_fold());
    assert_eq!(recurse(&lf, &lt, &(0, "x")), 6);
}

#[test]
fn trace_lift() {
    let lift = TraceLift;
    let lt = lift.lift_treeish(test_tree());
    let lf = lift.lift_fold(sum_fold());
    let result = recurse(&lf, &lt, &0);
    assert_eq!(result.orig, 6);
    assert_eq!(result.n_children, 2); // node 0 has 2 children
}

#[test]
fn compose_id_then_tag() {
    let c = ComposedLift::compose(Id, Tag("y"));
    let lt = c.lift_treeish(test_tree());
    let lf = c.lift_fold(sum_fold());
    assert_eq!(recurse(&lf, &lt, &c.lift_root(&0)), 6);
}

#[test]
fn compose_id_then_trace() {
    let c = ComposedLift::compose(Id, TraceLift);
    let lt = c.lift_treeish(test_tree());
    let lf = c.lift_fold(sum_fold());
    let result = recurse(&lf, &lt, &c.lift_root(&0));
    assert_eq!(result.orig, 6);
    assert_eq!(result.n_children, 2);
}

#[test]
fn compose_tag_then_trace() {
    // Tag changes N (i32 → (i32,&str)), then Trace wraps H/R.
    // Composed: N changes AND H/R change.
    let c = ComposedLift::compose(Tag("z"), TraceLift);
    let lt = c.lift_treeish(test_tree());
    let lf = c.lift_fold(sum_fold());
    let root = c.lift_root(&0);
    let result = recurse(&lf, &lt, &root);
    assert_eq!(result.orig, 6);
    assert_eq!(root, (0, "z"));
}

#[test]
fn compose_trace_then_tag() {
    // Trace wraps H/R first, then Tag changes N.
    // Tag is H/R-transparent, so it passes TraceHeap/TraceResult through.
    let c = ComposedLift::compose(TraceLift, Tag("w"));
    let lt = c.lift_treeish(test_tree());
    let lf = c.lift_fold(sum_fold());
    let root = c.lift_root(&0);
    let result = recurse(&lf, &lt, &root);
    assert_eq!(result.orig, 6);
    assert_eq!(root, (0, "w"));
}

#[test]
fn compose_three_lifts() {
    // Id ∘ Tag ∘ Trace — three lifts chained.
    let inner = ComposedLift::compose(Id, Tag("q"));
    let full = ComposedLift::compose(inner, TraceLift);
    let lt = full.lift_treeish(test_tree());
    let lf = full.lift_fold(sum_fold());
    let root = full.lift_root(&0);
    let result = recurse(&lf, &lt, &root);
    assert_eq!(result.orig, 6);
}
