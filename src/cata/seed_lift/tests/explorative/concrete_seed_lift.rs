//! Explorative — Lift with all four parameters at trait level, plain
//! associated types. Proves universal and concrete lifts and their
//! compositions compile with no unsafe.

use std::sync::Arc;
use std::marker::PhantomData;

// ── Minified carriers ───────────────────────────────

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
struct Treeish<N>(#[allow(dead_code)] Arc<dyn Fn(&N, &mut dyn FnMut(&N)) + Send + Sync>);

#[derive(Clone)]
struct Fold<N, H, R> { _p: PhantomData<fn() -> (N, H, R)> }
impl<N: 'static, H: 'static, R: 'static> Fold<N, H, R> {
    fn new() -> Self { Fold { _p: PhantomData } }
}

// ── The Lift type class ─────────────────────────────

trait Lift<N, Seed, H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2: Clone + 'static;
    type Seed2: Clone + 'static;
    type MapH: Clone + 'static;
    type MapR: Clone + 'static;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed2) -> Self::N2 + Send + Sync>,
            Edgy<Self::N2, Self::Seed2>,
            Treeish<Self::N2>,
            Fold<Self::N2, Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T;

    fn lift_root(&self, root: &N) -> Self::N2;
}

// ── IdentityLift ─────────────────────────────────────

#[derive(Clone, Copy)]
struct IdentityLift;

impl<N, Seed, H, R> Lift<N, Seed, H, R> for IdentityLift
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        cont(grow, seeds, treeish, fold)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── FilterSeedsLift — concrete Seed, polymorphic N/H/R ──

struct FilterSeedsLift<Seed, P> { pred: Arc<P>, _s: PhantomData<fn() -> Seed> }
impl<Seed, P> Clone for FilterSeedsLift<Seed, P> {
    fn clone(&self) -> Self { FilterSeedsLift { pred: self.pred.clone(), _s: PhantomData } }
}
fn filter_seeds_lift<Seed, P>(p: P) -> FilterSeedsLift<Seed, P>
where P: Fn(&Seed) -> bool + Send + Sync + 'static,
{ FilterSeedsLift { pred: Arc::new(p), _s: PhantomData } }

impl<N, Seed, H, R, P> Lift<N, Seed, H, R> for FilterSeedsLift<Seed, P>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      P: Fn(&Seed) -> bool + Send + Sync + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        _treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let pred = self.pred.clone();
        let filtered = seeds.filter(move |s: &Seed| pred(s));
        let dummy_tree = Treeish(Arc::new(|_: &N, _: &mut dyn FnMut(&N)| {}));
        cont(grow, filtered, dummy_tree, fold)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── WrapInitLift — concrete (N, H), polymorphic (Seed, R) ──

struct WrapInitLift<N, H, W> { wrapper: Arc<W>, _m: PhantomData<fn() -> (N, H)> }
impl<N, H, W> Clone for WrapInitLift<N, H, W> {
    fn clone(&self) -> Self { WrapInitLift { wrapper: self.wrapper.clone(), _m: PhantomData } }
}
fn wrap_init_lift<N, H, W>(w: W) -> WrapInitLift<N, H, W>
where W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
{ WrapInitLift { wrapper: Arc::new(w), _m: PhantomData } }

impl<N, Seed, H, R, W> Lift<N, Seed, H, R> for WrapInitLift<N, H, W>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
{
    type N2 = N;  type Seed2 = Seed;  type MapH = H;  type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        cont(grow, seeds, treeish, fold)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── TraceLift — polymorphic H/R wrap ──

#[derive(Clone)]
struct TraceHeap<N, H, R> { _p: PhantomData<fn() -> (N, H, R)> }
#[derive(Clone)]
struct TraceResult<N, H, R> { _p: PhantomData<fn() -> (N, H, R)> }

#[derive(Clone, Copy)]
struct TraceLift;

impl<N, Seed, H, R> Lift<N, Seed, H, R> for TraceLift
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;  type Seed2 = Seed;
    type MapH = TraceHeap<N, H, R>;
    type MapR = TraceResult<N, H, R>;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        _fold:   Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, TraceHeap<N, H, R>, TraceResult<N, H, R>>,
        ) -> T,
    ) -> T {
        cont(grow, seeds, treeish, Fold::new())
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

// ── ComposedLift ─────────────────────────────────────

struct ComposedLift<L1, L2> { inner: L1, outer: L2 }
impl<L1: Clone, L2: Clone> Clone for ComposedLift<L1, L2> {
    fn clone(&self) -> Self { ComposedLift { inner: self.inner.clone(), outer: self.outer.clone() } }
}

impl<N, Seed, H, R, L1, L2> Lift<N, Seed, H, R> for ComposedLift<L1, L2>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      L1: Lift<N, Seed, H, R>,
      L2: Lift<L1::N2, L1::Seed2, L1::MapH, L1::MapR>,
{
    type N2 = L2::N2;  type Seed2 = L2::Seed2;
    type MapH = L2::MapH;  type MapR = L2::MapR;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Self::Seed2) -> Self::N2 + Send + Sync>,
            Edgy<Self::N2, Self::Seed2>,
            Treeish<Self::N2>,
            Fold<Self::N2, Self::MapH, Self::MapR>,
        ) -> T,
    ) -> T {
        self.inner.apply(grow, seeds, treeish, fold, |g1, s1, t1, f1| {
            self.outer.apply(g1, s1, t1, f1, cont)
        })
    }

    fn lift_root(&self, root: &N) -> Self::N2 {
        self.outer.lift_root(&self.inner.lift_root(root))
    }
}

// ── Tests ──────────────────────────────────────────

fn empty_edgy<N: 'static, S: 'static>() -> Edgy<N, S> {
    Edgy(Arc::new(|_: &N, _: &mut dyn FnMut(&S)| {}))
}
fn empty_treeish<N: 'static>() -> Treeish<N> {
    Treeish(Arc::new(|_: &N, _: &mut dyn FnMut(&N)| {}))
}

#[test]
fn identity_polymorphic() {
    let id = IdentityLift;
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    <IdentityLift as Lift<i32, String, u64, u64>>::apply(
        &id, g, empty_edgy(), empty_treeish(), Fold::new(), |_, _, _, _| ());
}

#[test]
fn filter_seeds_concrete_no_unsafe() {
    let filt = filter_seeds_lift(|s: &String| !s.is_empty());
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    <FilterSeedsLift<String, _> as Lift<i32, String, u64, u64>>::apply(
        &filt, g, empty_edgy(), empty_treeish(), Fold::new(), |_, _, _, _| ());
}

#[test]
fn wrap_init_concrete() {
    let w = wrap_init_lift::<i32, u64, _>(|n: &i32, orig: &dyn Fn(&i32) -> u64| orig(n) * 2);
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    <WrapInitLift<i32, u64, _> as Lift<i32, String, u64, u64>>::apply(
        &w, g, empty_edgy(), empty_treeish(), Fold::new(), |_, _, _, _| ());
}

#[test]
fn trace_polymorphic_hr_wrap() {
    let t = TraceLift;
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    <TraceLift as Lift<i32, String, u64, u64>>::apply(
        &t, g, empty_edgy(), empty_treeish(), Fold::new(), |_, _, _, _| ());
}

#[test]
fn compose_filter_then_trace() {
    let c = ComposedLift { inner: filter_seeds_lift(|s: &String| !s.is_empty()), outer: TraceLift };
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    <ComposedLift<FilterSeedsLift<String, _>, TraceLift> as Lift<i32, String, u64, u64>>::apply(
        &c, g, empty_edgy(), empty_treeish(), Fold::new(), |_, _, _, _| ());
}

#[test]
fn compose_filter_wrapinit_trace() {
    let filt = filter_seeds_lift(|s: &String| !s.is_empty());
    let wi   = wrap_init_lift::<i32, u64, _>(|n, orig| orig(n) + 1);
    let inner = ComposedLift { inner: filt, outer: wi };
    let full  = ComposedLift { inner, outer: TraceLift };
    let g: Arc<dyn Fn(&String) -> i32 + Send + Sync> = Arc::new(|s| s.len() as i32);
    <ComposedLift<ComposedLift<FilterSeedsLift<String, _>, WrapInitLift<i32, u64, _>>, TraceLift>
        as Lift<i32, String, u64, u64>>::apply(
        &full, g, empty_edgy(), empty_treeish(), Fold::new(), |_, _, _, _| ());
}
