//! Explorative: extra bounds on Lift implementations.
//!
//! Companion to `bifunctor_lift.rs`. That file proved the bifunctor
//! shape (method-level R, two-parameter GATs, blanket `ComposedLift`).
//! This file probes the next question: **how does a power-user lift
//! whose machinery needs bounds stronger than the trait promises
//! carry those bounds without bifurcating the trait?**
//!
//! Findings (spoiler):
//!
//! PATH A — bound at USE, not CONSTRUCTION. **WORKS.**
//!   `lift_fold`'s body needs nothing beyond what the trait gives.
//!   Any extra bound lives on a SECONDARY method (eval, drive). The
//!   `Lift` impl's signature matches the trait verbatim. This is
//!   exactly the shape ParLazy already has; it can adopt `Lift`
//!   today with no trait change.
//!
//! PATH B — bound at CONSTRUCTION via GAT where-clauses.
//!   **DOES NOT WORK** in stable Rust. Adding `where R: Send` to
//!   a GAT's impl narrows the GAT's declared domain (H, R), which
//!   Rust treats as stricter-than-trait (E0276). See the Path B
//!   section below for the full negative result and the invasive
//!   alternatives (global bound / parallel trait hierarchy).
//!
//! Self-contained — minimal types redefined so this experiment is
//! independent of hylic's actual Lift codebase and survives future
//! refactors.

// ── The typing facts being probed ───────────────────
//
// FACT 1  A closure's Send/Sync is determined by its CAPTURED state.
//   Its RETURN type is irrelevant. `|| Rc::new(5)` is Send because
//   the closure value has no non-Send fields; the Rc is manufactured
//   at call time and stays on the calling thread.
//
// FACT 2  `Arc<dyn Fn(..) -> R + Send + Sync>` is Send + Sync
//   unconditionally, for any R. The `+ Send + Sync` in the trait
//   object is an auto-trait assertion; constructing one requires the
//   concrete Fn to satisfy it, but the erased object is Send + Sync
//   by contract thereafter.
//
// CONSEQUENCE for Fold: a `Fold<N, H, R>` whose fields are three such
//   Arcs is Send + Sync for every choice of N, H, R. R appears only
//   in function signatures, never as a stored value inside the struct.

use std::sync::{Arc, OnceLock};

// ── Minified Fold / Treeish / Lift ──────────────────

struct Fold<N, H, R> {
    init: Arc<dyn Fn(&N) -> H + Send + Sync>,
    acc: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    fin: Arc<dyn Fn(&H) -> R + Send + Sync>,
}

impl<N, H, R> Clone for Fold<N, H, R> {
    fn clone(&self) -> Self {
        Fold { init: self.init.clone(), acc: self.acc.clone(), fin: self.fin.clone() }
    }
}

impl<N: 'static, H: 'static, R: 'static> Fold<N, H, R> {
    fn new(
        init: impl Fn(&N) -> H + Send + Sync + 'static,
        acc: impl Fn(&mut H, &R) + Send + Sync + 'static,
        fin: impl Fn(&H) -> R + Send + Sync + 'static,
    ) -> Self {
        Fold { init: Arc::new(init), acc: Arc::new(acc), fin: Arc::new(fin) }
    }

    fn run(&self, node: &N, children: &[R]) -> R {
        let mut h = (self.init)(node);
        for c in children { (self.acc)(&mut h, c); }
        (self.fin)(&h)
    }
}

struct Treeish<N> { visit: Arc<dyn Fn(&N, &mut dyn FnMut(&N)) + Send + Sync> }

impl<N> Clone for Treeish<N> {
    fn clone(&self) -> Self { Treeish { visit: self.visit.clone() } }
}

impl<N: 'static> Treeish<N> {
    fn new(f: impl Fn(&N, &mut dyn FnMut(&N)) + Send + Sync + 'static) -> Self {
        Treeish { visit: Arc::new(f) }
    }
    fn collect(&self, node: &N) -> Vec<N> where N: Clone {
        let mut v = Vec::new();
        (self.visit)(node, &mut |c| v.push(c.clone()));
        v
    }
}

fn recurse<N: Clone + 'static, H: 'static, R: 'static>(
    fold: &Fold<N, H, R>, treeish: &Treeish<N>, node: &N,
) -> R {
    let children: Vec<R> = treeish.collect(node).iter()
        .map(|c| recurse(fold, treeish, c)).collect();
    fold.run(node, &children)
}

trait Lift<N: 'static, N2: 'static> {
    type MapH<H: Clone + 'static, R: Clone + 'static>: Clone + 'static;
    type MapR<H: Clone + 'static, R: Clone + 'static>: Clone + 'static;
    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N2>;
    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N2, Self::MapH<H, R>, Self::MapR<H, R>>;
    #[allow(dead_code)] // mirrors the full trait; unused in these tests
    fn lift_root(&self, root: &N) -> N2;
}

// ════════════════════════════════════════════════════
// PATH A — lazy-factory lift (the ParLazy analog)
// ════════════════════════════════════════════════════
//
// `lift_fold` builds a tree of `Arc<LazyNode>`; `eval` walks it.
// The construction phase never sends R values across threads — it
// only stores them in OnceLock slots that are filled later, during
// eval. The bound we need is `R: Send`, and it's needed in eval only.
//
// Per FACT 1 + FACT 2, the three construction closures are Send +
// Sync without R: Send:
//   - each closure captures (at most) a `Fold<N,H,R>` clone — which
//     is Send+Sync unconditionally
//   - each closure returns a type that mentions R — irrelevant to
//     the closure's own auto-traits
// → The Lift impl's signature matches the trait verbatim.

struct LazyNode<H, R> {
    heap: H,
    children: Vec<Arc<LazyNode<H, R>>>,
    result: OnceLock<R>,
}

struct LazyHeap<H, R> {
    heap: H,
    children: Vec<Arc<LazyNode<H, R>>>,
}

impl<H: Clone, R> Clone for LazyHeap<H, R> {
    fn clone(&self) -> Self {
        LazyHeap { heap: self.heap.clone(), children: self.children.clone() }
    }
}

struct LazyResult<N, H, R> {
    node: Arc<LazyNode<H, R>>,
    fold: Fold<N, H, R>,
}

impl<N, H, R> Clone for LazyResult<N, H, R> {
    fn clone(&self) -> Self {
        LazyResult { node: self.node.clone(), fold: self.fold.clone() }
    }
}

struct Lazy;

impl<N: Clone + 'static> Lift<N, N> for Lazy {
    type MapH<H: Clone + 'static, R: Clone + 'static> = LazyHeap<H, R>;
    type MapR<H: Clone + 'static, R: Clone + 'static> = LazyResult<N, H, R>;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }
    fn lift_root(&self, root: &N) -> N { root.clone() }

    // Signature identical to the trait — no `where R: Send`.
    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: Fold<N, H, R>,
    ) -> Fold<N, LazyHeap<H, R>, LazyResult<N, H, R>> {
        let f1 = f.clone();
        let f2 = f;
        Fold::new(
            // init: captures f1 (Fold, Send+Sync). Returns LazyHeap — irrelevant.
            move |n: &N| LazyHeap { heap: (f1.init)(n), children: Vec::new() },
            // accumulate: non-move, no captures. Trivially Send+Sync.
            |heap: &mut LazyHeap<H, R>, child: &LazyResult<N, H, R>| {
                heap.children.push(child.node.clone());
            },
            // finalize: captures f2 (Fold, Send+Sync). Returns LazyResult — irrelevant.
            move |heap: &LazyHeap<H, R>| LazyResult {
                node: Arc::new(LazyNode {
                    heap: heap.heap.clone(),
                    children: heap.children.clone(),
                    result: OnceLock::new(),
                }),
                fold: f2.clone(),
            },
        )
    }
}

impl Lazy {
    // The genuine `R: Send` bound lives here, on a secondary method.
    // This is where R values cross threads in the real ParLazy.
    fn eval<N, H, R>(&self, result: LazyResult<N, H, R>) -> R
    where
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + Send + 'static,
    {
        eval_node(&result.node, &result.fold)
    }
}

fn eval_node<N, H: Clone, R: Clone + Send>(
    node: &Arc<LazyNode<H, R>>, fold: &Fold<N, H, R>,
) -> R {
    if let Some(r) = node.result.get() { return r.clone(); }
    // Real impl fans out via thread::spawn / rayon::join here —
    // that's where R: Send becomes load-bearing. The experiment
    // walks sequentially since the typing story is what's under
    // test, not the pool arithmetic.
    let children: Vec<R> = node.children.iter()
        .map(|c| eval_node(c, fold)).collect();
    let mut heap = node.heap.clone();
    for r in &children { (fold.acc)(&mut heap, r); }
    let r = (fold.fin)(&heap);
    let _ = node.result.set(r.clone());
    r
}

// ── Tests ──────────────────────────────────────────

fn test_tree() -> Treeish<i32> {
    let ch: Vec<Vec<i32>> = vec![vec![1, 2], vec![3], vec![], vec![]];
    Treeish::new(move |n: &i32, cb: &mut dyn FnMut(&i32)| {
        if let Some(children) = ch.get(*n as usize) {
            for &c in children { cb(&c); }
        }
    })
}

fn sum_fold() -> Fold<i32, i64, i64> {
    Fold::new(|n: &i32| *n as i64, |h: &mut i64, c: &i64| *h += c, |h: &i64| *h)
}

#[test]
fn path_a_builds_and_evals_with_send_r() {
    let lift = Lazy;
    let lt = lift.lift_treeish(test_tree());
    let lf = lift.lift_fold(sum_fold());
    let lazy_result = recurse(&lf, &lt, &0);
    assert_eq!(lift.eval(lazy_result), 6);
}

// ── Payoff: non-Send R passes construction ─────────
//
// `Rc<u64>` is not Send. Path A's Lift impl accepts it anyway —
// construction is bound-free. The Send requirement surfaces only
// where it becomes real, at eval.

use std::rc::Rc;

fn rc_fold() -> Fold<i32, u64, Rc<u64>> {
    // init / acc / fin closures: no captures, or captures that are
    // trivially Send. Returning Rc<u64> is not captured — per FACT 1
    // it doesn't touch the closure's auto-traits.
    Fold::new(
        |n: &i32| *n as u64,
        |h: &mut u64, c: &Rc<u64>| *h += **c,
        |h: &u64| Rc::new(*h),
    )
}

#[test]
fn path_a_construction_compiles_with_non_send_r() {
    // R = Rc<u64>. The Lift trait asks only Clone + 'static — satisfied.
    let lift = Lazy;
    let _lt = lift.lift_treeish(test_tree());
    let _lf: Fold<i32, LazyHeap<u64, Rc<u64>>, LazyResult<i32, u64, Rc<u64>>>
        = lift.lift_fold(rc_fold());

    // If we tried `lift.eval(_lazy_result)` on a LazyResult<_,_,Rc<u64>>
    // the compiler would reject it:
    //
    //   error[E0277]: `Rc<u64>` cannot be sent between threads safely
    //     = note: required by a bound in `Lazy::eval`
    //
    // Correct layering: construction is bound-free; the extra bound
    // surfaces precisely at the method that needs it.
}

// ════════════════════════════════════════════════════
// PATH B — construction genuinely needs the bound
// ════════════════════════════════════════════════════
//
// What if `lift_fold`'s body can't be bound-free? (E.g. it eagerly
// spawns work during construction, or stores children in a
// concurrent cache that requires R: Send.)
//
// The tempting move is to narrow the GAT:
//
//     impl<N: Clone + 'static> Lift<N, N> for EagerLift {
//         type MapH<H: Clone + 'static, R: Clone + 'static> = EagerHeap<H, R>
//             where R: Send;                    // ◀── ATTEMPT
//         type MapR<H: Clone + 'static, R: Clone + 'static> = R
//             where R: Send;                    // ◀── ATTEMPT
//         fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
//             &self, f: Fold<N, H, R>,
//         ) -> Fold<N, Self::MapH<H, R>, Self::MapR<H, R>> { /* R: Send body */ }
//     }
//
// EMPIRICALLY FALSIFIED. Stable Rust rejects it:
//
//     error[E0276]: impl has stricter requirements than trait
//        |
//    .. | type MapH<H: Clone + 'static, R: Clone + 'static>: Clone + 'static;
//        |     -------------------------------------------------------------
//        |         definition of `MapH` from trait
//    .. | type MapH<H: Clone + 'static, R: Clone + 'static> = ... where R: Send;
//        |                                                              ^^^^
//        |                                    impl has extra requirement `R: Send`
//
// Rust treats GAT `where` clauses on IMPL sites as narrowing the
// GAT's declared domain (H: Clone + 'static, R: Clone + 'static).
// Narrowing is stricter-than-trait, regardless of whether the bound
// would propagate cleanly to callers via well-formedness. There is
// no path from `R: Clone + 'static` to `R: Send` in Rust's implied-
// bounds rules — `Send` is not implied by anything declared on the
// trait, so the impl can't introduce it without E0276.
//
// We tried the obvious move and it failed. Recording the failure is
// the point of this experiment.
//
// ── What actually works for Path B today ──
//
// Only invasive options remain:
//
//   (1) Strengthen the Lift trait globally. Add `R: Send` (or
//       whatever bound is needed) to the trait's method/GATs. Every
//       lift, including bound-free ones, now demands it. Simple, but
//       global.
//
//   (2) Parallel trait hierarchy. A separate `SendLift<N, N2>` whose
//       GATs and method carry `R: Clone + Send + 'static`. Lifts that
//       NEED construction bounds implement `SendLift`; bound-free
//       lifts implement `Lift`. Composition bifurcates: `ComposedLift`
//       gets multiple impls keyed on which variants are combined.
//
//   (3) The lift opts out of the trait. Standalone methods (as
//       ParLazy did before this experiment). Loses `apply_pre_lift`
//       composition.
//
// For hylic's actual scope: every lift we have (Explainer, SeedLift,
// IdentityLift, ParLazy) fits Path A. Option (1) is tempting as a
// safety net but loses non-Send R support for sequential use. Option
// (2) is available if a future power-user lift genuinely hits the
// construction-bound case.
//
// Until then: the experiment says **Path A is the universal solution
// for the cases we actually have**. Use the trait as-is, put extra
// bounds on secondary methods, and document the layering.

// ════════════════════════════════════════════════════
// Composition commentary
// ════════════════════════════════════════════════════
//
// `ComposedLift<L1, L2, Nmid>`'s blanket `Lift<N, N2>` impl (see
// `bifunctor_lift.rs`) projects `MapR<H, R> = L2::MapR<L1::MapH<H,R>,
// L1::MapR<H,R>>`. Because every L1 and L2 we compose obeys Path A,
// no extra bound on R ever enters the composition. Each lift's
// secondary method (eval, drive, run_*) carries its own bounds at
// USE, and the chain's caller picks them up where the chain is
// finally driven — at the executor.
