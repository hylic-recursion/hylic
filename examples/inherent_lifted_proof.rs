//! Proof: inherent run_lifted on FusedIn<D>, fully domain-generic.
//! No Shared-specific code. Clone bound on the domain's fold/treeish.

use std::marker::PhantomData;
use std::sync::Arc;
use std::rc::Rc;

// ── Operations traits ─────────────────────────────

trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}

trait TreeOps<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N));
}

// ── Domain trait ──────────────────────────────────

trait Domain<N: 'static>: 'static {
    type Fold<H: 'static, R: 'static>: FoldOps<N, H, R>;
    type Treeish: TreeOps<N>;
}

// ── LiftOps trait ─────────────────────────────────

trait LiftOps<D, N: 'static, H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static>
where D: Domain<N> + Domain<N2>
{
    fn lift_treeish(&self, t: <D as Domain<N>>::Treeish) -> <D as Domain<N2>>::Treeish;
    fn lift_fold(&self, f: <D as Domain<N>>::Fold<H, R>) -> <D as Domain<N2>>::Fold<H2, R2>;
    fn lift_root(&self, root: &N) -> N2;
    fn unwrap(&self, result: R2) -> R;
}

// ── FusedIn<D> with INHERENT run + run_lifted ─────

struct FusedIn<D>(PhantomData<D>);
impl<D> Copy for FusedIn<D> {}
impl<D> Clone for FusedIn<D> { fn clone(&self) -> Self { *self } }

impl<D> FusedIn<D> {
    // Inherent run — no trait import needed
    pub fn run<N: 'static, H: 'static, R: 'static>(
        &self,
        fold: &<D as Domain<N>>::Fold<H, R>,
        graph: &<D as Domain<N>>::Treeish,
        root: &N,
    ) -> R
    where D: Domain<N>
    {
        let mut heap = fold.init(root);
        graph.visit(root, &mut |child: &N| {
            let r = self.run(fold, graph, child);
            fold.accumulate(&mut heap, &r);
        });
        fold.finalize(&heap)
    }

    // Inherent run_lifted — DOMAIN-GENERIC, not Shared-specific
    pub fn run_lifted<N: 'static, R: 'static, N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> R0
    where
        D: Domain<N> + Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }
}

// ── Shared domain ─────────────────────────────────

struct Shared;

struct SharedFold<N, H, R> {
    init_fn: Arc<dyn Fn(&N) -> H + Send + Sync>,
    acc_fn: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    fin_fn: Arc<dyn Fn(&H) -> R + Send + Sync>,
}
impl<N, H, R> Clone for SharedFold<N, H, R> {
    fn clone(&self) -> Self {
        SharedFold { init_fn: self.init_fn.clone(), acc_fn: self.acc_fn.clone(), fin_fn: self.fin_fn.clone() }
    }
}
impl<N, H, R> FoldOps<N, H, R> for SharedFold<N, H, R> {
    fn init(&self, n: &N) -> H { (self.init_fn)(n) }
    fn accumulate(&self, h: &mut H, r: &R) { (self.acc_fn)(h, r) }
    fn finalize(&self, h: &H) -> R { (self.fin_fn)(h) }
}

struct SharedTreeish<N> { visit_fn: Arc<dyn Fn(&N, &mut dyn FnMut(&N)) + Send + Sync> }
impl<N> Clone for SharedTreeish<N> { fn clone(&self) -> Self { SharedTreeish { visit_fn: self.visit_fn.clone() } } }
impl<N> TreeOps<N> for SharedTreeish<N> {
    fn visit(&self, n: &N, cb: &mut dyn FnMut(&N)) { (self.visit_fn)(n, cb) }
}

impl<N: 'static> Domain<N> for Shared {
    type Fold<H: 'static, R: 'static> = SharedFold<N, H, R>;
    type Treeish = SharedTreeish<N>;
}

// ── Local domain ──────────────────────────────────

struct Local;

struct LocalFold<N, H, R> {
    init_fn: Rc<dyn Fn(&N) -> H>,
    acc_fn: Rc<dyn Fn(&mut H, &R)>,
    fin_fn: Rc<dyn Fn(&H) -> R>,
}
impl<N, H, R> Clone for LocalFold<N, H, R> {
    fn clone(&self) -> Self {
        LocalFold { init_fn: self.init_fn.clone(), acc_fn: self.acc_fn.clone(), fin_fn: self.fin_fn.clone() }
    }
}
impl<N, H, R> FoldOps<N, H, R> for LocalFold<N, H, R> {
    fn init(&self, n: &N) -> H { (self.init_fn)(n) }
    fn accumulate(&self, h: &mut H, r: &R) { (self.acc_fn)(h, r) }
    fn finalize(&self, h: &H) -> R { (self.fin_fn)(h) }
}

struct LocalTreeish<N> { visit_fn: Rc<dyn Fn(&N, &mut dyn FnMut(&N))> }
impl<N> Clone for LocalTreeish<N> { fn clone(&self) -> Self { LocalTreeish { visit_fn: self.visit_fn.clone() } } }
impl<N> TreeOps<N> for LocalTreeish<N> {
    fn visit(&self, n: &N, cb: &mut dyn FnMut(&N)) { (self.visit_fn)(n, cb) }
}

impl<N: 'static> Domain<N> for Local {
    type Fold<H: 'static, R: 'static> = LocalFold<N, H, R>;
    type Treeish = LocalTreeish<N>;
}

// ── A trivial Lift (identity) for testing ─────────

struct IdentityLift<D>(PhantomData<D>);

impl<D, N: Clone + 'static, H: 'static, R: 'static> LiftOps<D, N, H, R, N, H, R> for IdentityLift<D>
where D: Domain<N>
{
    fn lift_treeish(&self, t: <D as Domain<N>>::Treeish) -> <D as Domain<N>>::Treeish { t }
    fn lift_fold(&self, f: <D as Domain<N>>::Fold<H, R>) -> <D as Domain<N>>::Fold<H, R> { f }
    fn lift_root(&self, root: &N) -> N { root.clone() }
    fn unwrap(&self, result: R) -> R { result }
}

// ── Consts ────────────────────────────────────────

const FUSED_SHARED: FusedIn<Shared> = FusedIn(PhantomData);
const FUSED_LOCAL: FusedIn<Local> = FusedIn(PhantomData);

// ── Tests ─────────────────────────────────────────

fn main() {
    // Shared: run
    let fold = SharedFold {
        init_fn: Arc::new(|n: &i32| *n as u64),
        acc_fn: Arc::new(|h: &mut u64, r: &u64| *h += r),
        fin_fn: Arc::new(|h: &u64| *h),
    };
    let graph = SharedTreeish { visit_fn: Arc::new(|_: &i32, _: &mut dyn FnMut(&i32)| {}) };
    let r = FUSED_SHARED.run(&fold, &graph, &42);
    assert_eq!(r, 42);
    println!("Shared run: {}", r);

    // Shared: run_lifted with identity lift
    let r2 = FUSED_SHARED.run_lifted(&IdentityLift::<Shared>(PhantomData), &fold, &graph, &42);
    assert_eq!(r2, 42);
    println!("Shared run_lifted: {}", r2);

    // Local: run
    let local_fold = LocalFold {
        init_fn: Rc::new(|n: &i32| *n as u64),
        acc_fn: Rc::new(|h: &mut u64, r: &u64| *h += r),
        fin_fn: Rc::new(|h: &u64| *h),
    };
    let local_graph = LocalTreeish { visit_fn: Rc::new(|_: &i32, _: &mut dyn FnMut(&i32)| {}) };
    let r3 = FUSED_LOCAL.run(&local_fold, &local_graph, &42);
    assert_eq!(r3, 42);
    println!("Local run: {}", r3);

    // Local: run_lifted with identity lift — SAME method, different domain
    let r4 = FUSED_LOCAL.run_lifted(&IdentityLift::<Local>(PhantomData), &local_fold, &local_graph, &42);
    assert_eq!(r4, 42);
    println!("Local run_lifted: {}", r4);

    println!("All passed. No trait imports. No Shared-specific code.");
}
