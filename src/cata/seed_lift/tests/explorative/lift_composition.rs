//! Explorative: lift composition as function composition on type-level
//! transformations. Self-contained — no dependency on hylic's types.
//!
//! The core problem: given two lifts L1 and L2 where L1's output feeds
//! into L2's input, compose them into a single lift. The difficulty is
//! that L1's output types (LiftedH, LiftedR) are GATs depending on a
//! method-level parameter H. L2 must accept these as input.
//!
//! Solution: OuterLift<Inner> — a trait parameterized by the inner lift
//! at the trait level, so the outer lift's methods can reference the
//! inner's GATs. This is contraposition: the outer lift is defined
//! "against" the inner lift's type structure.
//!
//! In FP terms: if Lift is a higher-kinded functor on the algebra,
//! OuterLift is the contravariant composition slot. ComposedLift is
//! the functor composition L2 ∘ L1.

use std::marker::PhantomData;
use std::sync::Arc;

// ── Minimal Fold and Treeish (self-contained) ───────

/// Minimal fold: three closures.
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

/// Minimal treeish: callback child visitor.
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

    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N)) { (self.visit)(node, cb) }

    fn collect(&self, node: &N) -> Vec<N> where N: Clone {
        let mut v = Vec::new();
        self.visit(node, &mut |c| v.push(c.clone()));
        v
    }

    fn treemap<N2: Clone + 'static>(
        &self,
        co: impl Fn(&N) -> N2 + 'static,
        contra: impl Fn(&N2) -> N + 'static,
    ) -> Treeish<N2> {
        let inner = self.visit.clone();
        Treeish::new(move |n: &N2, cb: &mut dyn FnMut(&N2)| {
            inner(&contra(n), &mut |child: &N| { let mapped = co(child); cb(&mapped); });
        })
    }
}

/// Recursive fold execution (minimal fused executor).
fn recurse<N: Clone + 'static, H: 'static, R: 'static>(fold: &Fold<N, H, R>, treeish: &Treeish<N>, node: &N) -> R {
    let children: Vec<R> = treeish.collect(node).iter()
        .map(|c| recurse(fold, treeish, c))
        .collect();
    fold.run(node, &children)
}

// ── LiftOps trait ───────────────────────────────────

trait LiftOps<N: 'static, R: 'static, N2: 'static> {
    type LiftedH<H: Clone + 'static>: Clone + 'static;
    type LiftedR<H: Clone + 'static>: Clone + 'static;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2>;
    fn lift_fold<H: Clone + 'static>(&self, f: Fold<N, H, R>) -> Fold<N2, Self::LiftedH<H>, Self::LiftedR<H>>;
    fn lift_root(&self, root: &N) -> N2;
}

// ── OuterLift: contraposition composition slot ──────
//
// In FP: if L1 is a lift (endofunctor on algebras), OuterLift<L1>
// is a natural transformation that operates on L1's output. The
// trait-level L1 parameter makes L1's GATs (LiftedH, LiftedR)
// available to the outer lift's method signatures.

trait OuterLift<Inner, N: 'static, R: 'static, Nmid: Clone + 'static, N2: 'static>
where
    Inner: LiftOps<N, R, Nmid>,
{
    type LiftedH<H: Clone + 'static>: Clone + 'static;
    type LiftedR<H: Clone + 'static>: Clone + 'static;

    fn lift_treeish(&self, t: Treeish<Nmid>) -> Treeish<N2>;
    fn lift_fold<H: Clone + 'static>(
        &self,
        f: Fold<Nmid, Inner::LiftedH<H>, Inner::LiftedR<H>>,
    ) -> Fold<N2, Self::LiftedH<H>, Self::LiftedR<H>>;
    fn lift_root(&self, root: &Nmid) -> N2;
}

// ── ComposedLift: L2 ∘ L1 ──────────────────────────
//
// Functor composition. L1 transforms first, L2 transforms
// L1's output. Implements LiftOps for the composed pair.

struct ComposedLift<L1, L2, Nmid> {
    inner: L1,
    outer: L2,
    _mid: PhantomData<fn() -> Nmid>,
}

impl<L1, L2, Nmid> ComposedLift<L1, L2, Nmid> {
    fn compose(inner: L1, outer: L2) -> Self {
        ComposedLift { inner, outer, _mid: PhantomData }
    }
}

impl<N, R, Nmid, N2, L1, L2> LiftOps<N, R, N2> for ComposedLift<L1, L2, Nmid>
where
    N: Clone + 'static,
    R: Clone + 'static,
    Nmid: Clone + 'static,
    N2: Clone + 'static,
    L1: LiftOps<N, R, Nmid>,
    L2: OuterLift<L1, N, R, Nmid, N2>,
{
    type LiftedH<H: Clone + 'static> = L2::LiftedH<H>;
    type LiftedR<H: Clone + 'static> = L2::LiftedR<H>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2> {
        self.outer.lift_treeish(self.inner.lift_treeish(t))
    }

    fn lift_fold<H: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N2, Self::LiftedH<H>, Self::LiftedR<H>> {
        self.outer.lift_fold(self.inner.lift_fold(f))
    }

    fn lift_root(&self, root: &N) -> N2 {
        self.outer.lift_root(&self.inner.lift_root(root))
    }
}

// ── Concrete lifts ──────────────────────────────────

/// Identity lift. Passes everything through.
struct Id;

impl<N: Clone + 'static, R: Clone + 'static> LiftOps<N, R, N> for Id {
    type LiftedH<H: Clone + 'static> = H;
    type LiftedR<H: Clone + 'static> = R;
    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }
    fn lift_fold<H: Clone + 'static>(&self, f: Fold<N, H, R>) -> Fold<N, H, R> { f }
    fn lift_root(&self, root: &N) -> N { root.clone() }
}

impl<Inner, N, R, Nmid> OuterLift<Inner, N, R, Nmid, Nmid> for Id
where
    N: 'static, R: Clone + 'static, Nmid: Clone + 'static,
    Inner: LiftOps<N, R, Nmid>,
{
    type LiftedH<H: Clone + 'static> = Inner::LiftedH<H>;
    type LiftedR<H: Clone + 'static> = Inner::LiftedR<H>;
    fn lift_treeish(&self, t: Treeish<Nmid>) -> Treeish<Nmid> { t }
    fn lift_fold<H: Clone + 'static>(
        &self, f: Fold<Nmid, Inner::LiftedH<H>, Inner::LiftedR<H>>,
    ) -> Fold<Nmid, Inner::LiftedH<H>, Inner::LiftedR<H>> { f }
    fn lift_root(&self, root: &Nmid) -> Nmid { root.clone() }
}

/// Tag lift. Wraps N as (N, &'static str). N-changing, H/R transparent.
struct TagLift(&'static str);

impl<R: Clone + 'static> LiftOps<i32, R, (i32, &'static str)> for TagLift {
    type LiftedH<H: Clone + 'static> = H;
    type LiftedR<H: Clone + 'static> = R;
    fn lift_treeish(&self, t: Treeish<i32>) -> Treeish<(i32, &'static str)> {
        let tag = self.0;
        t.treemap(move |n: &i32| (*n, tag), |pair: &(i32, &str)| pair.0)
    }
    fn lift_fold<H: Clone + 'static>(&self, f: Fold<i32, H, R>) -> Fold<(i32, &'static str), H, R> {
        f.contramap(|pair: &(i32, &'static str)| pair.0)
    }
    fn lift_root(&self, root: &i32) -> (i32, &'static str) { (*root, self.0) }
}

impl<Inner, N, R> OuterLift<Inner, N, R, i32, (i32, &'static str)> for TagLift
where
    N: 'static, R: Clone + 'static,
    Inner: LiftOps<N, R, i32>,
{
    type LiftedH<H: Clone + 'static> = Inner::LiftedH<H>;
    type LiftedR<H: Clone + 'static> = Inner::LiftedR<H>;
    fn lift_treeish(&self, t: Treeish<i32>) -> Treeish<(i32, &'static str)> {
        let tag = self.0;
        t.treemap(move |n: &i32| (*n, tag), |pair: &(i32, &str)| pair.0)
    }
    fn lift_fold<H: Clone + 'static>(
        &self, f: Fold<i32, Inner::LiftedH<H>, Inner::LiftedR<H>>,
    ) -> Fold<(i32, &'static str), Inner::LiftedH<H>, Inner::LiftedR<H>> {
        f.contramap(|pair: &(i32, &'static str)| pair.0)
    }
    fn lift_root(&self, root: &i32) -> (i32, &'static str) { (*root, self.0) }
}

// ── Test fixtures ───────────────────────────────────

fn test_tree() -> Treeish<i32> {
    // 0→[1,2], 1→[3], 2→[], 3→[]
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
    assert_eq!(recurse(&sum_fold(), &test_tree(), &0), 6); // 0+1+2+3
}

#[test]
fn identity_lift() {
    let lift = Id;
    let lt = LiftOps::<i32, i64, i32>::lift_treeish(&lift, test_tree());
    let lf = LiftOps::<i32, i64, i32>::lift_fold(&lift, sum_fold());
    assert_eq!(recurse(&lf, &lt, &0), 6);
}

#[test]
fn tag_lift_standalone() {
    let lift = TagLift("x");
    let lt = LiftOps::<i32, i64, _>::lift_treeish(&lift, test_tree());
    let lf = LiftOps::<i32, i64, _>::lift_fold(&lift, sum_fold());
    assert_eq!(recurse(&lf, &lt, &(0, "x")), 6);
}

#[test]
fn compose_id_then_tag() {
    let c = ComposedLift::<_, _, i32>::compose(Id, TagLift("y"));
    let lt = LiftOps::<i32, i64, (i32, &str)>::lift_treeish(&c, test_tree());
    let lf = LiftOps::<i32, i64, (i32, &str)>::lift_fold(&c, sum_fold());
    let root = LiftOps::<i32, i64, (i32, &str)>::lift_root(&c, &0);
    assert_eq!(recurse(&lf, &lt, &root), 6);
}

#[test]
fn compose_tag_then_id() {
    let c = ComposedLift::<_, _, (i32, &str)>::compose(TagLift("z"), Id);
    let lt = LiftOps::<i32, i64, (i32, &str)>::lift_treeish(&c, test_tree());
    let lf = LiftOps::<i32, i64, (i32, &str)>::lift_fold(&c, sum_fold());
    let root = LiftOps::<i32, i64, (i32, &str)>::lift_root(&c, &0);
    assert_eq!(recurse(&lf, &lt, &root), 6);
}

// ── Type inference tests ────────────────────────────
//
// Can the compiler infer types when composition is used in a
// context that constrains the result?

#[test]
fn inferred_from_fold() {
    // R is inferred from sum_fold() → Fold<i32, i64, i64>, so R = i64.
    // Nmid inferred from Id (i32→i32) and TagLift (i32→(i32,&str)).
    let c = ComposedLift::compose(Id, TagLift("inferred"));
    let lf: Fold<(i32, &str), i64, i64> = LiftOps::lift_fold(&c, sum_fold());
    // Once R is known from lift_fold, lift_treeish resolves:
    let lt = LiftOps::<i32, i64, (i32, &str)>::lift_treeish(&c, test_tree());
    let root = LiftOps::<i32, i64, (i32, &str)>::lift_root(&c, &0);
    assert_eq!(recurse(&lf, &lt, &root), 6);
    assert_eq!(root, (0, "inferred"));
}
