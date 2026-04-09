//! Proof-of-concept: domain-parameterized executors with GATs.
//!
//! Verifies that:
//! 1. FusedIn<D> blanket impl compiles (GAT bounds propagate to impl FoldOps)
//! 2. Inference works (no turbofish needed at call sites)
//! 3. Domain-incompatible combinations are rejected at compile time

use std::marker::PhantomData;

// ── Operations traits ─────────────────────────────

trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}

trait TreeOps<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N));
}

// ── Domain trait with GATs ────────────────────────

trait Domain<N: 'static>: 'static {
    type Fold<H: 'static, R: 'static>: FoldOps<N, H, R>;
    type Treeish: TreeOps<N>;
}

// ── Two concrete domains ──────────────────────────

pub struct Shared;
pub struct Owned;

// Shared domain: Arc-based (simulated with a simple wrapper)
struct SharedFold<N, H, R> {
    init_fn: Box<dyn Fn(&N) -> H + Send + Sync>,
    acc_fn: Box<dyn Fn(&mut H, &R) + Send + Sync>,
    fin_fn: Box<dyn Fn(&H) -> R + Send + Sync>,
}

impl<N, H, R> FoldOps<N, H, R> for SharedFold<N, H, R> {
    fn init(&self, n: &N) -> H { (self.init_fn)(n) }
    fn accumulate(&self, h: &mut H, r: &R) { (self.acc_fn)(h, r) }
    fn finalize(&self, h: &H) -> R { (self.fin_fn)(h) }
}

struct SharedTreeish<N> {
    visit_fn: Box<dyn Fn(&N, &mut dyn FnMut(&N)) + Send + Sync>,
}

impl<N> TreeOps<N> for SharedTreeish<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N)) { (self.visit_fn)(node, cb) }
}

impl<N: 'static> Domain<N> for Shared {
    type Fold<H: 'static, R: 'static> = SharedFold<N, H, R>;
    type Treeish = SharedTreeish<N>;
}

// Owned domain: Box-based, no Send+Sync
struct OwnedFold<N, H, R> {
    init_fn: Box<dyn Fn(&N) -> H>,
    acc_fn: Box<dyn Fn(&mut H, &R)>,
    fin_fn: Box<dyn Fn(&H) -> R>,
}

impl<N, H, R> FoldOps<N, H, R> for OwnedFold<N, H, R> {
    fn init(&self, n: &N) -> H { (self.init_fn)(n) }
    fn accumulate(&self, h: &mut H, r: &R) { (self.acc_fn)(h, r) }
    fn finalize(&self, h: &H) -> R { (self.fin_fn)(h) }
}

struct OwnedTreeish<N> {
    visit_fn: Box<dyn Fn(&N, &mut dyn FnMut(&N))>,
}

impl<N> TreeOps<N> for OwnedTreeish<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N)) { (self.visit_fn)(node, cb) }
}

impl<N: 'static> Domain<N> for Owned {
    type Fold<H: 'static, R: 'static> = OwnedFold<N, H, R>;
    type Treeish = OwnedTreeish<N>;
}

// ── Executor trait ────────────────────────────────

trait Executor<N: 'static, R: 'static, D: Domain<N>> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R;
}

// ── FusedIn<D>: blanket impl over all domains ─────

pub struct FusedIn<D>(PhantomData<D>);
impl<D> Copy for FusedIn<D> {}
impl<D> Clone for FusedIn<D> { fn clone(&self) -> Self { *self } }
impl<D> Default for FusedIn<D> { fn default() -> Self { FusedIn(PhantomData) } }

// THE KEY: one blanket impl. D is fixed by the type parameter.
impl<N: 'static, R: 'static, D: Domain<N>> Executor<N, R, D> for FusedIn<D> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        // D::Fold<H,R>: FoldOps<N,H,R> — guaranteed by Domain trait.
        // D::Treeish: TreeOps<N> — guaranteed by Domain trait.
        // Can we pass them to a generic function?
        fused_recurse(fold, graph, root)
    }
}

// Generic recursion engine — takes impl FoldOps + impl TreeOps
fn fused_recurse<N, H, R>(
    fold: &impl FoldOps<N, H, R>,
    graph: &impl TreeOps<N>,
    node: &N,
) -> R {
    let mut heap = fold.init(node);
    graph.visit(node, &mut |child: &N| {
        let r = fused_recurse(fold, graph, child);
        fold.accumulate(&mut heap, &r);
    });
    fold.finalize(&heap)
}

// ── Flattened executors ──────────────────────────
// Type aliases for signatures + generics:
#[allow(dead_code)] type Fused = FusedIn<Shared>;
#[allow(dead_code)] type FusedOwned = FusedIn<Owned>;

// Constants for use as values — zero-sized, const-constructible:
#[allow(non_upper_case_globals)]
pub const fused: FusedIn<Shared> = FusedIn(PhantomData);
#[allow(non_upper_case_globals)]
pub const fused_owned: FusedIn<Owned> = FusedIn(PhantomData);

// ── Test ──────────────────────────────────────────

fn main() {
    // Shared fold + Shared treeish
    let shared_fold = SharedFold {
        init_fn: Box::new(|n: &i32| *n as u64),
        acc_fn: Box::new(|h: &mut u64, r: &u64| *h += r),
        fin_fn: Box::new(|h: &u64| *h),
    };
    let shared_graph = SharedTreeish {
        visit_fn: Box::new(|_n: &i32, _cb: &mut dyn FnMut(&i32)| {
            // leaf node — no children
        }),
    };

    // Owned fold + Owned treeish
    let owned_fold = OwnedFold {
        init_fn: Box::new(|n: &i32| *n as u64),
        acc_fn: Box::new(|h: &mut u64, r: &u64| *h += r),
        fin_fn: Box::new(|h: &u64| *h),
    };
    let owned_graph = OwnedTreeish {
        visit_fn: Box::new(|_n: &i32, _cb: &mut dyn FnMut(&i32)| {}),
    };

    // TEST 1: const value
    let r1 = fused.run(&shared_fold, &shared_graph, &42);
    println!("via const: {}", r1);

    // TEST 2: Default::default() through type alias — THE ERGONOMIC WIN
    let r2 = Fused::default().run(&shared_fold, &shared_graph, &42);
    println!("via Fused::default(): {}", r2);

    // TEST 3: different domain via const
    let r3 = fused_owned.run(&owned_fold, &owned_graph, &42);
    println!("via fused_owned const: {}", r3);

    // TEST 4: different domain via Default
    let r4 = FusedOwned::default().run(&owned_fold, &owned_graph, &42);
    println!("via FusedOwned::default(): {}", r4);

    // TEST 5: type mismatch — should NOT compile (uncomment to verify)
    // let r5 = Fused::default().run(&owned_fold, &owned_graph, &42);
    //                                ^^^^^^^^^^^ expected SharedFold, got OwnedFold

    assert_eq!(r1, 42);
    assert_eq!(r2, 42);
    assert_eq!(r3, 42);
    assert_eq!(r4, 42);
    println!("All tests passed.");
}
