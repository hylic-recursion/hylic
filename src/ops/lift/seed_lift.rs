//! SeedLift — the finishing Lift that retires the Seed axis.
//!
//! `SeedLift<D, N, Seed, H>` is a `Lift<D, N, H, R>` with
//! `N2 = LiftedNode<N>`, `MapH = H`, `MapR = R`. It carries
//! Entry-dispatch state as struct fields:
//!
//!   - `grow`: the pipeline's Seed→N resolver (D-stored).
//!   - `entry_seeds`: `Edgy<(), Seed>` — canonical callback-iterator.
//!     Convenience wrappers lower `&[Seed]` into this shape.
//!   - `entry_heap_fn`: produces the root heap at Entry (D-stored).
//!
//! Traversal:
//!   - `LiftedNode::Entry.visit` → fans out to
//!     `LiftedNode::Node(grow(seed))` for each entry seed.
//!   - `LiftedNode::Node(n).visit` → delegates to the base treeish;
//!     emits children wrapped as `LiftedNode::Node(child)`.
//!
//! The fold is wrapped identically for both variants — Node uses
//! the base fold's init/accumulate/finalize; Entry uses
//! `entry_heap_fn()` for init and the base fold's accumulate +
//! finalize otherwise.
//!
//! Domain-parametric: the struct is one, but the `apply` body
//! differs per domain (Shared's closures need `Send + Sync`; Local's
//! do not). The `Lift` impl is therefore written once per domain
//! via the domain's own fold/graph constructors.

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use crate::domain::{Domain, Shared, Local};
use crate::graph::{self, Edgy, Treeish};
use super::lifted_node::{LiftedNode, LiftedNodeInner};
use super::core::Lift;

// ANCHOR: seed_lift_struct
/// The finishing lift that closes a `SeedPipeline`'s grow axis.
/// Composes entry-seed dispatch on top of a `(grow, seeds, fold)`
/// triple and produces a treeish over `LiftedNode<N>`. Not
/// user-constructed; assembled internally by
/// `LiftedSeedPipeline::run` at call time.
///
/// Parametric in a `Domain` so the same struct shape is reused
/// across `Shared` (Arc-backed, `Send + Sync`) and `Local`
/// (Rc-backed, `!Send`). Each domain ships its own `Lift` impl
/// (below). Custom domains could add their own impl against this
/// struct and the library's `Lift` trait.
#[must_use]
pub struct SeedLift<D, N, Seed, H>
where D: Domain<N>,
      N: 'static, Seed: 'static, H: 'static,
{
    pub(crate) grow:          <D as Domain<N>>::Grow<Seed, N>,
    pub(crate) entry_seeds:   Edgy<(), Seed>,
    pub(crate) entry_heap_fn: EntryHeapFn<D, H>,
    _m: PhantomData<fn() -> (D, N, Seed, H)>,
}
// ANCHOR_END: seed_lift_struct

/// Per-domain storage for the `entry_heap_fn` closure. Parallel to
/// `<D as Domain<_>>::Grow<(), H>` in shape, but kept separate
/// because that GAT demands a `Fn(&()) -> H`, while the library
/// wants `Fn() -> H` (no seed reference). Arc/Rc concrete per D.
#[doc(hidden)]
pub enum EntryHeapFn<D, H>
where D: 'static, H: 'static,
{
    #[doc(hidden)]
    Shared(Arc<dyn Fn() -> H + Send + Sync>),
    #[doc(hidden)]
    Local(Rc<dyn Fn() -> H>),
    #[doc(hidden)]
    _Phantom(PhantomData<fn() -> D>, std::convert::Infallible),
}

impl<D, H> Clone for EntryHeapFn<D, H>
where D: 'static, H: 'static,
{
    fn clone(&self) -> Self {
        match self {
            EntryHeapFn::Shared(a) => EntryHeapFn::Shared(a.clone()),
            EntryHeapFn::Local(r)  => EntryHeapFn::Local(r.clone()),
            EntryHeapFn::_Phantom(_, x) => match *x {},
        }
    }
}

impl<D, N, Seed, H> Clone for SeedLift<D, N, Seed, H>
where D: Domain<N>,
      <D as Domain<N>>::Grow<Seed, N>: Clone,
      N: 'static, Seed: 'static, H: 'static,
{
    fn clone(&self) -> Self {
        SeedLift {
            grow:          self.grow.clone(),
            entry_seeds:   self.entry_seeds.clone(),
            entry_heap_fn: self.entry_heap_fn.clone(),
            _m: PhantomData,
        }
    }
}

// ── Shared constructors ──────────────────────────────────────

impl<N, Seed, H> SeedLift<Shared, N, Seed, H>
where N: 'static, Seed: 'static, H: 'static,
{
    /// Public constructor (Shared) for user-explicit composition
    /// (`then_lift(SeedLift::new(...))`).
    pub fn new<F, HFn>(
        grow:          F,
        entry_seeds:   Edgy<(), Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where F:   Fn(&Seed) -> N + Send + Sync + 'static,
          HFn: Fn()        -> H + Send + Sync + 'static,
    {
        SeedLift {
            grow:          Arc::new(grow),
            entry_seeds,
            entry_heap_fn: EntryHeapFn::Shared(Arc::new(entry_heap_fn)),
            _m: PhantomData,
        }
    }

    /// Construct from a pre-built `Arc<Fn>` grow closure without
    /// re-wrapping. Used by `hylic-pipeline`'s `LiftedSeedPipeline::run`
    /// which already holds an `Arc`.
    pub fn from_arc_grow<HFn>(
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        entry_seeds: Edgy<(), Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where HFn: Fn() -> H + Send + Sync + 'static,
    {
        SeedLift {
            grow,
            entry_seeds,
            entry_heap_fn: EntryHeapFn::Shared(Arc::new(entry_heap_fn)),
            _m: PhantomData,
        }
    }
}

// ── Local constructors ───────────────────────────────────────

impl<N, Seed, H> SeedLift<Local, N, Seed, H>
where N: 'static, Seed: 'static, H: 'static,
{
    /// Public constructor (Local) for user-explicit composition.
    /// Closures need not be `Send + Sync`.
    pub fn new_local<F, HFn>(
        grow:          F,
        entry_seeds:   Edgy<(), Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where F:   Fn(&Seed) -> N + 'static,
          HFn: Fn()        -> H + 'static,
    {
        SeedLift {
            grow:          Rc::new(grow),
            entry_seeds,
            entry_heap_fn: EntryHeapFn::Local(Rc::new(entry_heap_fn)),
            _m: PhantomData,
        }
    }

    /// Construct from a pre-built `Rc<Fn>` grow closure. Used by the
    /// Local-domain `LiftedSeedPipeline::run` once available.
    pub fn from_rc_grow<HFn>(
        grow: Rc<dyn Fn(&Seed) -> N>,
        entry_seeds: Edgy<(), Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where HFn: Fn() -> H + 'static,
    {
        SeedLift {
            grow,
            entry_seeds,
            entry_heap_fn: EntryHeapFn::Local(Rc::new(entry_heap_fn)),
            _m: PhantomData,
        }
    }
}

// ── Shared Lift impl ─────────────────────────────────────────

impl<N, Seed, H, R> Lift<Shared, N, H, R> for SeedLift<Shared, N, Seed, H>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2   = LiftedNode<N>;
    type MapH = H;
    type MapR = R;

    fn apply<T>(
        &self,
        treeish: <Shared as Domain<N>>::Graph<N>,
        fold:    <Shared as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <Shared as Domain<LiftedNode<N>>>::Graph<LiftedNode<N>>,
            <Shared as Domain<LiftedNode<N>>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        use crate::domain::shared::fold::{self as sfold, Fold};

        // ── lifted treeish ─────────────────────────────────
        let sl_grow     = self.grow.clone();
        let entry_seeds = self.entry_seeds.clone();
        let base        = treeish;
        let lifted_treeish: Treeish<LiftedNode<N>> = graph::treeish_visit(
            move |n: &LiftedNode<N>, cb: &mut dyn FnMut(&LiftedNode<N>)| match &n.inner {
                LiftedNodeInner::Node(node) => {
                    base.visit(node, &mut |c: &N| cb(&LiftedNode::node(c.clone())));
                }
                LiftedNodeInner::Entry => entry_seeds.visit(&(),
                    &mut |s: &Seed| cb(&LiftedNode::node(sl_grow(s)))),
            },
        );

        // ── lifted fold ────────────────────────────────────
        let heap_fn = match &self.entry_heap_fn {
            EntryHeapFn::Shared(a) => a.clone(),
            EntryHeapFn::Local(_)  => unreachable!("Shared SeedLift cannot carry Local entry_heap_fn"),
            EntryHeapFn::_Phantom(_, x) => match *x {},
        };
        let f1 = fold.clone(); let f2 = fold.clone(); let f3 = fold;
        let lifted_fold: Fold<LiftedNode<N>, H, R> = sfold::fold(
            move |n: &LiftedNode<N>| match &n.inner {
                LiftedNodeInner::Node(node) => f1.init(node),
                LiftedNodeInner::Entry      => heap_fn(),
            },
            move |heap: &mut H, result: &R| f2.accumulate(heap, result),
            move |heap: &H| f3.finalize(heap),
        );

        cont(lifted_treeish, lifted_fold)
    }
}

// ── Local Lift impl ──────────────────────────────────────────

impl<N, Seed, H, R> Lift<Local, N, H, R> for SeedLift<Local, N, Seed, H>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2   = LiftedNode<N>;
    type MapH = H;
    type MapR = R;

    fn apply<T>(
        &self,
        treeish: <Local as Domain<N>>::Graph<N>,
        fold:    <Local as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <Local as Domain<LiftedNode<N>>>::Graph<LiftedNode<N>>,
            <Local as Domain<LiftedNode<N>>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        use crate::domain::local::{self, edgy as local_edgy};

        // ── lifted treeish ─────────────────────────────────
        let sl_grow     = self.grow.clone();
        let entry_seeds = self.entry_seeds.clone();
        let base        = treeish;
        let lifted_treeish: local_edgy::Treeish<LiftedNode<N>> =
            local_edgy::treeish_visit(
                move |n: &LiftedNode<N>, cb: &mut dyn FnMut(&LiftedNode<N>)| match &n.inner {
                    LiftedNodeInner::Node(node) => {
                        base.visit(node, &mut |c: &N| cb(&LiftedNode::node(c.clone())));
                    }
                    LiftedNodeInner::Entry => entry_seeds.visit(&(),
                        &mut |s: &Seed| cb(&LiftedNode::node(sl_grow(s)))),
                },
            );

        // ── lifted fold ────────────────────────────────────
        let heap_fn = match &self.entry_heap_fn {
            EntryHeapFn::Local(r)   => r.clone(),
            EntryHeapFn::Shared(_)  => unreachable!("Local SeedLift cannot carry Shared entry_heap_fn"),
            EntryHeapFn::_Phantom(_, x) => match *x {},
        };
        let f1 = fold.clone(); let f2 = fold.clone(); let f3 = fold;
        let lifted_fold: local::Fold<LiftedNode<N>, H, R> = local::fold(
            move |n: &LiftedNode<N>| match &n.inner {
                LiftedNodeInner::Node(node) => f1.init(node),
                LiftedNodeInner::Entry      => heap_fn(),
            },
            move |heap: &mut H, result: &R| f2.accumulate(heap, result),
            move |heap: &H| f3.finalize(heap),
        );

        cont(lifted_treeish, lifted_fold)
    }
}
