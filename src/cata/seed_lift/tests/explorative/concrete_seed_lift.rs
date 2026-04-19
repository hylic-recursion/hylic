//! Explorative — solving the concrete-Seed shape-lift problem.
//!
//! Self-contained. The prior CPS Lift design had Seed as a method-
//! level generic, which meant a `FilterSeedsLift<Seed0, P>` carrying a
//! `Fn(&Seed0) -> bool` couldn't match the method's polymorphic `Seed`.
//!
//! The fix: **move `Seed` to the trait level**, keep N/H/R
//! method-level. The bifunctor-on-(H, R) and functor-on-N character
//! is preserved (GATs for `N2<N>`, `MapH<N,H,R>`, `MapR<N,H,R>`).
//! `Seed2` becomes a plain associated type (since Seed is now
//! trait-level too).
//!
//! This file VERIFIES the design compiles for every relevant lift
//! shape: Identity, FilterSeeds (concrete-Seed closure), N-Preserving
//! H/R wrap (Explainer-like), and ComposedLift.

use std::sync::Arc;
use std::marker::PhantomData;

// ── Minified Fold / Edgy ────────────────────────────

#[derive(Clone)]
struct Edgy<N, E>(#[allow(dead_code)] Arc<dyn Fn(&N, &mut dyn FnMut(&E)) + Send + Sync>);

impl<N: 'static, E: 'static> Edgy<N, E> {
    fn filter<P: Fn(&E) -> bool + Send + Sync + 'static>(self, p: P) -> Self {
        let inner = self.0.clone();
        Edgy(Arc::new(move |n: &N, cb: &mut dyn FnMut(&E)| {
            inner(n, &mut |e: &E| { if p(e) { cb(e); } });
        }))
    }
}

#[derive(Clone)]
struct Fold<N, H, R> {
    _p: PhantomData<fn() -> (N, H, R)>,
}

impl<N: 'static, H: 'static, R: 'static> Fold<N, H, R> {
    fn new() -> Self { Fold { _p: PhantomData } }
}

// ── The revised Lift trait: Seed at trait level ─────

trait Lift<Seed: Clone + 'static> {
    type N2<N: Clone + 'static>: Clone + 'static;
    type Seed2: Clone + 'static;   // plain associated type now
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>: Clone + 'static;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>: Clone + 'static;

    fn apply<N, H, R, T>(
        &self,
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds: Edgy<N, Seed>,
        fold: Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed2) -> Self::N2<N> + Send + Sync>,
            Edgy<Self::N2<N>, Self::Seed2>,
            Fold<Self::N2<N>, Self::MapH<N, H, R>, Self::MapR<N, H, R>>,
        ) -> T,
    ) -> T
    where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static;

    fn lift_root<N: Clone + 'static>(&self, root: &N) -> Self::N2<N>;
}

// ── IdentityLift ─────────────────────────────────────

#[derive(Clone, Copy)]
struct IdentityLift;

impl<Seed: Clone + 'static> Lift<Seed> for IdentityLift {
    type N2<N: Clone + 'static> = N;
    type Seed2 = Seed;
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static> = H;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static> = R;

    fn apply<N, H, R, T>(
        &self,
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds: Edgy<N, Seed>,
        fold: Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T
    where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        cont(grow, seeds, fold)
    }

    fn lift_root<N: Clone + 'static>(&self, r: &N) -> N { r.clone() }
}

// ── FilterSeedsLift — concrete-Seed closure ─────────
//
// THIS IS THE TEST CASE. The predicate is typed to Seed at trait
// level. Apply's method-level N, H, R are polymorphic. Seed is NOT
// a method generic — it's fixed by the trait impl.

struct FilterSeedsLift<Seed, P> {
    pred: Arc<P>,
    _s: PhantomData<fn() -> Seed>,
}

impl<Seed, P> Clone for FilterSeedsLift<Seed, P> {
    fn clone(&self) -> Self { FilterSeedsLift { pred: self.pred.clone(), _s: PhantomData } }
}

fn filter_seeds_lift<Seed, P>(pred: P) -> FilterSeedsLift<Seed, P>
where P: Fn(&Seed) -> bool + Send + Sync + 'static,
{
    FilterSeedsLift { pred: Arc::new(pred), _s: PhantomData }
}

impl<Seed, P> Lift<Seed> for FilterSeedsLift<Seed, P>
where Seed: Clone + 'static,
      P: Fn(&Seed) -> bool + Send + Sync + 'static,
{
    type N2<N: Clone + 'static> = N;
    type Seed2 = Seed;
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static> = H;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static> = R;

    fn apply<N, H, R, T>(
        &self,
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds: Edgy<N, Seed>,
        fold: Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T
    where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        // No transmute. The closure's &Seed matches the method's Seed
        // because Seed is the SAME type parameter at both levels —
        // it's on the trait. The N that varies is polymorphic here.
        let pred = self.pred.clone();
        let filtered = seeds.filter(move |s: &Seed| pred(s));
        cont(grow, filtered, fold)
    }

    fn lift_root<N: Clone + 'static>(&self, r: &N) -> N { r.clone() }
}

// ── Explainer-like: H/R wrap, polymorphic in N ──────

#[derive(Clone)]
struct TraceHeap<N, H, R> { _p: PhantomData<fn() -> (N, H, R)> }
#[derive(Clone)]
struct TraceResult<N, H, R> { _p: PhantomData<fn() -> (N, H, R)> }

#[derive(Clone, Copy)]
struct TraceLift;

impl<Seed: Clone + 'static> Lift<Seed> for TraceLift {
    type N2<N: Clone + 'static> = N;
    type Seed2 = Seed;
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>
        = TraceHeap<N, H, R>;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>
        = TraceResult<N, H, R>;

    fn apply<N, H, R, T>(
        &self,
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds: Edgy<N, Seed>,
        _fold: Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Fold<N, TraceHeap<N, H, R>, TraceResult<N, H, R>>,
        ) -> T,
    ) -> T
    where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        let wrapped_fold: Fold<N, TraceHeap<N, H, R>, TraceResult<N, H, R>> = Fold::new();
        cont(grow, seeds, wrapped_fold)
    }

    fn lift_root<N: Clone + 'static>(&self, r: &N) -> N { r.clone() }
}

// ── ComposedLift — CPS-nested apply, Seed threads through ──

struct ComposedLift<L1, L2> {
    inner: L1,
    outer: L2,
}

impl<L1: Clone, L2: Clone> Clone for ComposedLift<L1, L2> {
    fn clone(&self) -> Self { ComposedLift { inner: self.inner.clone(), outer: self.outer.clone() } }
}

impl<Seed, L1, L2> Lift<Seed> for ComposedLift<L1, L2>
where
    Seed: Clone + 'static,
    L1: Lift<Seed>,
    L2: Lift<L1::Seed2>,
{
    type N2<N: Clone + 'static> = L2::N2<L1::N2<N>>;
    type Seed2 = L2::Seed2;
    type MapH<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>
        = L2::MapH<L1::N2<N>, L1::MapH<N, H, R>, L1::MapR<N, H, R>>;
    type MapR<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>
        = L2::MapR<L1::N2<N>, L1::MapH<N, H, R>, L1::MapR<N, H, R>>;

    fn apply<N, H, R, T>(
        &self,
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds: Edgy<N, Seed>,
        fold: Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed2) -> Self::N2<N> + Send + Sync>,
            Edgy<Self::N2<N>, Self::Seed2>,
            Fold<Self::N2<N>, Self::MapH<N, H, R>, Self::MapR<N, H, R>>,
        ) -> T,
    ) -> T
    where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        self.inner.apply(grow, seeds, fold, |g1, s1, f1| {
            // g1: Arc<Fn(&L1::Seed2) -> L1::N2<N>>
            // s1: Edgy<L1::N2<N>, L1::Seed2>
            // f1: Fold<L1::N2<N>, L1::MapH<N,H,R>, L1::MapR<N,H,R>>
            // outer is Lift<L1::Seed2>, so its apply takes Seed = L1::Seed2.
            self.outer.apply(g1, s1, f1, cont)
        })
    }

    fn lift_root<N: Clone + 'static>(&self, r: &N) -> Self::N2<N> {
        self.outer.lift_root(&self.inner.lift_root(r))
    }
}

// ── Tests: every lift shape compiles and composes ────

fn empty_edgy<N: 'static, S: 'static>() -> Edgy<N, S> {
    Edgy(Arc::new(|_: &N, _: &mut dyn FnMut(&S)| {}))
}

#[test]
fn identity_compiles() {
    let id = IdentityLift;
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s: &String| s.len() as i32);
    Lift::<String>::apply(&id, g, empty_edgy(), Fold::<i32, u64, u64>::new(), |_, _, _| ());
}

#[test]
fn filter_seeds_compiles() {
    // The main test: a shape-lift with a concrete-Seed closure
    // compiles as a Lift<Seed> impl with no transmute.
    let filt = filter_seeds_lift(|s: &String| !s.is_empty());
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    Lift::<String>::apply(&filt, g, empty_edgy(), Fold::<i32, u64, u64>::new(), |_, _, _| ());
}

#[test]
fn compose_id_then_filter() {
    let c = ComposedLift { inner: IdentityLift, outer: filter_seeds_lift(|s: &String| !s.is_empty()) };
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    Lift::<String>::apply(&c, g, empty_edgy(), Fold::<i32, u64, u64>::new(), |_, _, _| ());
}

#[test]
fn compose_filter_then_trace() {
    let c = ComposedLift { inner: filter_seeds_lift(|s: &String| !s.is_empty()), outer: TraceLift };
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    Lift::<String>::apply(&c, g, empty_edgy(), Fold::<i32, u64, u64>::new(), |_, _, _| ());
}

#[test]
fn compose_trace_then_filter() {
    // Outer filter needs Seed = L1::Seed2 = String (TraceLift's Seed2 = Seed).
    let c = ComposedLift { inner: TraceLift, outer: filter_seeds_lift(|s: &String| !s.is_empty()) };
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    Lift::<String>::apply(&c, g, empty_edgy(), Fold::<i32, u64, u64>::new(), |_, _, _| ());
}

#[test]
fn three_deep_chain() {
    let inner = ComposedLift { inner: IdentityLift, outer: TraceLift };
    let full = ComposedLift { inner, outer: filter_seeds_lift(|s: &String| !s.is_empty()) };
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    Lift::<String>::apply(&full, g, empty_edgy(), Fold::<i32, u64, u64>::new(), |_, _, _| ());
}
