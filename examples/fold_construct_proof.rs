//! Proof: FoldConstruct trait enables domain-generic transformations.
//!
//! The key question: can `map` clone the fold, capture clones in
//! new closures, and pass them to `construct()` — all generically?

use std::rc::Rc;
use std::sync::Arc;

// ── FoldOps (the operations trait) ────────────────

trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}

// ── FoldConstruct (the domain-generic builder) ────

trait FoldConstruct<N, H, R>: FoldOps<N, H, R> + Clone + 'static {
    /// A fold of the same domain but with different type params.
    type Mapped<N2: 'static, H2: 'static, R2: 'static>: FoldConstruct<N2, H2, R2>;

    /// Construct a new fold in this domain.
    fn construct<N2: 'static, H2: 'static, R2: 'static>(
        init: impl Fn(&N2) -> H2 + 'static,
        acc: impl Fn(&mut H2, &R2) + 'static,
        fin: impl Fn(&H2) -> R2 + 'static,
    ) -> Self::Mapped<N2, H2, R2>;
}

// ── Shared Fold (Arc-based) ───────────────────────

struct SharedFold<N, H, R> {
    init_fn: Arc<dyn Fn(&N) -> H>,
    acc_fn: Arc<dyn Fn(&mut H, &R)>,
    fin_fn: Arc<dyn Fn(&H) -> R>,
}

impl<N, H, R> Clone for SharedFold<N, H, R> {
    fn clone(&self) -> Self {
        SharedFold {
            init_fn: self.init_fn.clone(),
            acc_fn: self.acc_fn.clone(),
            fin_fn: self.fin_fn.clone(),
        }
    }
}

impl<N, H, R> FoldOps<N, H, R> for SharedFold<N, H, R> {
    fn init(&self, n: &N) -> H { (self.init_fn)(n) }
    fn accumulate(&self, h: &mut H, r: &R) { (self.acc_fn)(h, r) }
    fn finalize(&self, h: &H) -> R { (self.fin_fn)(h) }
}

impl<N: 'static, H: 'static, R: 'static> FoldConstruct<N, H, R> for SharedFold<N, H, R> {
    type Mapped<N2: 'static, H2: 'static, R2: 'static> = SharedFold<N2, H2, R2>;

    fn construct<N2: 'static, H2: 'static, R2: 'static>(
        init: impl Fn(&N2) -> H2 + 'static,
        acc: impl Fn(&mut H2, &R2) + 'static,
        fin: impl Fn(&H2) -> R2 + 'static,
    ) -> SharedFold<N2, H2, R2> {
        SharedFold { init_fn: Arc::new(init), acc_fn: Arc::new(acc), fin_fn: Arc::new(fin) }
    }
}

// ── Local Fold (Rc-based) ─────────────────────────

struct LocalFold<N, H, R> {
    init_fn: Rc<dyn Fn(&N) -> H>,
    acc_fn: Rc<dyn Fn(&mut H, &R)>,
    fin_fn: Rc<dyn Fn(&H) -> R>,
}

impl<N, H, R> Clone for LocalFold<N, H, R> {
    fn clone(&self) -> Self {
        LocalFold {
            init_fn: self.init_fn.clone(),
            acc_fn: self.acc_fn.clone(),
            fin_fn: self.fin_fn.clone(),
        }
    }
}

impl<N, H, R> FoldOps<N, H, R> for LocalFold<N, H, R> {
    fn init(&self, n: &N) -> H { (self.init_fn)(n) }
    fn accumulate(&self, h: &mut H, r: &R) { (self.acc_fn)(h, r) }
    fn finalize(&self, h: &H) -> R { (self.fin_fn)(h) }
}

impl<N: 'static, H: 'static, R: 'static> FoldConstruct<N, H, R> for LocalFold<N, H, R> {
    type Mapped<N2: 'static, H2: 'static, R2: 'static> = LocalFold<N2, H2, R2>;

    fn construct<N2: 'static, H2: 'static, R2: 'static>(
        init: impl Fn(&N2) -> H2 + 'static,
        acc: impl Fn(&mut H2, &R2) + 'static,
        fin: impl Fn(&H2) -> R2 + 'static,
    ) -> LocalFold<N2, H2, R2> {
        LocalFold { init_fn: Rc::new(init), acc_fn: Rc::new(acc), fin_fn: Rc::new(fin) }
    }
}

// ── Generic transformation: map ───────────────────
// Written ONCE. Works for SharedFold AND LocalFold.

fn fold_map<F, N, H, R, RNew>(
    fold: &F,
    mapper: impl Fn(&R) -> RNew + 'static,
    backmapper: impl Fn(&RNew) -> R + 'static,
) -> F::Mapped<N, H, RNew>
where
    F: FoldConstruct<N, H, R>,
    N: 'static, H: 'static, R: 'static, RNew: 'static,
{
    let f1 = fold.clone();
    let f2 = fold.clone();
    let f3 = fold.clone();
    F::construct(
        move |node: &N| f1.init(node),
        move |heap: &mut H, result: &RNew| f2.accumulate(heap, &backmapper(result)),
        move |heap: &H| mapper(&f3.finalize(heap)),
    )
}

// ── Generic transformation: contramap ─────────────

fn fold_contramap<F, N, H, R, NewN>(
    fold: &F,
    f: impl Fn(&NewN) -> N + 'static,
) -> F::Mapped<NewN, H, R>
where
    F: FoldConstruct<N, H, R>,
    N: 'static, H: 'static, R: 'static, NewN: 'static,
{
    let f1 = fold.clone();
    let f2 = fold.clone();
    let f3 = fold.clone();
    F::construct(
        move |new_n: &NewN| f1.init(&f(&new_n)),
        move |h: &mut H, r: &R| f2.accumulate(h, r),
        move |h: &H| f3.finalize(h),
    )
}

// ── Test ──────────────────────────────────────────

fn main() {
    // Shared fold: sum
    let shared = SharedFold {
        init_fn: Arc::new(|n: &i32| *n as u64),
        acc_fn: Arc::new(|h: &mut u64, r: &u64| *h += r),
        fin_fn: Arc::new(|h: &u64| *h),
    };

    // map: u64 → String
    let mapped: SharedFold<i32, u64, String> = fold_map(
        &shared,
        |r: &u64| format!("result={}", r),
        |s: &String| s.strip_prefix("result=").unwrap().parse().unwrap(),
    );
    let mut heap = mapped.init(&42);
    mapped.accumulate(&mut heap, &"result=10".to_string());
    let result = mapped.finalize(&heap);
    println!("Shared mapped: {}", result);
    assert_eq!(result, "result=52");

    // Same transformation on Local fold
    let local = LocalFold {
        init_fn: Rc::new(|n: &i32| *n as u64),
        acc_fn: Rc::new(|h: &mut u64, r: &u64| *h += r),
        fin_fn: Rc::new(|h: &u64| *h),
    };

    let mapped_local: LocalFold<i32, u64, String> = fold_map(
        &local,
        |r: &u64| format!("result={}", r),
        |s: &String| s.strip_prefix("result=").unwrap().parse().unwrap(),
    );
    let mut heap = mapped_local.init(&42);
    mapped_local.accumulate(&mut heap, &"result=10".to_string());
    let result = mapped_local.finalize(&heap);
    println!("Local mapped: {}", result);
    assert_eq!(result, "result=52");

    // contramap: change node type from &str → i32 lookup
    let contramapped: SharedFold<&str, u64, u64> = fold_contramap(
        &shared,
        |s: &&str| s.len() as i32,
    );
    let mut heap = contramapped.init(&"hello");
    let result = contramapped.finalize(&heap);
    println!("Contramapped: {}", result);
    assert_eq!(result, 5); // "hello".len() = 5

    println!("All tests passed.");
}
