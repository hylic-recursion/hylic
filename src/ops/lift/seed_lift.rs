//! SeedLift — the finishing Lift that retires the Seed axis.
//!
//! `SeedLift<D, N, Seed, H>` is a `Lift<D, N, H, R>` with
//! `N2 = SeedNode<N>`, `MapH = H`, `MapR = R`. It carries
//! Entry-dispatch state as struct fields:
//!
//!   - `grow`: the pipeline's Seed→N resolver (D-stored).
//!   - `entry_seeds`: the seed callback-iterator over `()`,
//!     stored in the pipeline's domain (per-domain `Graph<Seed>`).
//!   - `entry_heap_fn`: produces the root heap at Entry, stored via
//!     the per-domain `ShapeCapable::EntryHeap<H>` GAT (Arc on
//!     Shared, Rc on Local).
//!
//! Traversal:
//!   - `SeedNode::EntryRoot.visit` → fans out to
//!     `SeedNode::Node(grow(seed))` for each entry seed.
//!   - `SeedNode::Node(n).visit` → delegates to the base treeish;
//!     emits children wrapped as `SeedNode::Node(child)`.
//!
//! The fold is wrapped identically for both variants — Node uses
//! the base fold's init/accumulate/finalize; EntryRoot uses
//! `entry_heap_fn()` for init and the base fold's accumulate +
//! finalize otherwise.
//!
//! Domain-parametric: the struct shape is one. The `Lift` impl is
//! written once per domain because the fold-construction closures
//! capture domain-typed state (Arc on Shared, Rc on Local) and the
//! per-domain fold constructors (`shared::fold::fold`,
//! `local::fold`) avoid the (a-uniform) `Send + Sync` bound on
//! `Domain::make_fold`'s closure inputs.

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use crate::domain::{Domain, Shared, Local};
use crate::graph::Treeish;
use super::seed_node::{SeedNode, SeedNodeInner};
use super::core::Lift;
use super::capability::ShapeCapable;

// ANCHOR: seed_lift_struct
/// The finishing lift that closes a `SeedPipeline`'s grow axis.
/// Composes entry-seed dispatch on top of a `(grow, seeds, fold)`
/// triple and produces a treeish over `SeedNode<N>`. Not
/// user-constructed; assembled internally by
/// `Stage2Pipeline::run` at call time.
///
/// Domain-parametric: storage of the entry-seeds graph and the
/// entry-heap thunk is per-domain via `<D as Domain<()>>::Graph<Seed>`
/// and `<D as ShapeCapable<N>>::EntryHeap<H>`. No hand-rolled
/// domain discriminator.
#[must_use]
pub struct SeedLift<D, N, Seed, H>
where D: ShapeCapable<N> + Domain<()>,
      N: 'static, Seed: 'static, H: 'static,
{
    pub(crate) grow:          <D as Domain<N>>::Grow<Seed, N>,
    pub(crate) entry_seeds:   <D as Domain<()>>::Graph<Seed>,
    pub(crate) entry_heap_fn: <D as ShapeCapable<N>>::EntryHeap<H>,
    _m: PhantomData<fn() -> (D, N, Seed, H)>,
}
// ANCHOR_END: seed_lift_struct

impl<D, N, Seed, H> Clone for SeedLift<D, N, Seed, H>
where D: ShapeCapable<N> + Domain<()>,
      <D as Domain<N>>::Grow<Seed, N>: Clone,
      <D as Domain<()>>::Graph<Seed>:  Clone,
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
        entry_seeds:   <Shared as Domain<()>>::Graph<Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where F:   Fn(&Seed) -> N + Send + Sync + 'static,
          HFn: Fn()        -> H + Send + Sync + 'static,
    {
        SeedLift {
            grow:          Arc::new(grow),
            entry_seeds,
            entry_heap_fn: Arc::new(entry_heap_fn),
            _m: PhantomData,
        }
    }

    /// Construct from a pre-built `Arc<Fn>` grow closure without
    /// re-wrapping. Used by `hylic-pipeline`'s seeded-run impl
    /// which already holds an `Arc`.
    pub fn from_arc_grow<HFn>(
        grow:        Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        entry_seeds: <Shared as Domain<()>>::Graph<Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where HFn: Fn() -> H + Send + Sync + 'static,
    {
        SeedLift {
            grow,
            entry_seeds,
            entry_heap_fn: Arc::new(entry_heap_fn),
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
        entry_seeds:   <Local as Domain<()>>::Graph<Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where F:   Fn(&Seed) -> N + 'static,
          HFn: Fn()        -> H + 'static,
    {
        SeedLift {
            grow:          Rc::new(grow),
            entry_seeds,
            entry_heap_fn: Rc::new(entry_heap_fn),
            _m: PhantomData,
        }
    }

    /// Construct from a pre-built `Rc<Fn>` grow closure. Used by the
    /// Local-domain seeded-run impl.
    pub fn from_rc_grow<HFn>(
        grow:        Rc<dyn Fn(&Seed) -> N>,
        entry_seeds: <Local as Domain<()>>::Graph<Seed>,
        entry_heap_fn: HFn,
    ) -> Self
    where HFn: Fn() -> H + 'static,
    {
        SeedLift {
            grow,
            entry_seeds,
            entry_heap_fn: Rc::new(entry_heap_fn),
            _m: PhantomData,
        }
    }
}

// ── Shared Lift impl ─────────────────────────────────────────

impl<N, Seed, H, R> Lift<Shared, N, H, R> for SeedLift<Shared, N, Seed, H>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2   = SeedNode<N>;
    type MapH = H;
    type MapR = R;

    fn apply<T>(
        &self,
        treeish: <Shared as Domain<N>>::Graph<N>,
        fold:    <Shared as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <Shared as Domain<SeedNode<N>>>::Graph<SeedNode<N>>,
            <Shared as Domain<SeedNode<N>>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        use crate::graph::{self};
        use crate::domain::shared::fold::{self as sfold, Fold};

        // ── lifted treeish ─────────────────────────────────
        let sl_grow     = self.grow.clone();
        let entry_seeds = self.entry_seeds.clone();
        let base        = treeish;
        let lifted_treeish: Treeish<SeedNode<N>> = graph::treeish_visit(
            move |n: &SeedNode<N>, cb: &mut dyn FnMut(&SeedNode<N>)| match &n.inner {
                SeedNodeInner::Node(node) => {
                    base.visit(node, &mut |c: &N| cb(&SeedNode::node(c.clone())));
                }
                SeedNodeInner::EntryRoot => entry_seeds.visit(&(),
                    &mut |s: &Seed| cb(&SeedNode::node(sl_grow(s)))),
            },
        );

        // ── lifted fold ────────────────────────────────────
        let heap_fn = self.entry_heap_fn.clone();
        let f1 = fold.clone(); let f2 = fold.clone(); let f3 = fold;
        let lifted_fold: Fold<SeedNode<N>, H, R> = sfold::fold(
            move |n: &SeedNode<N>| match &n.inner {
                SeedNodeInner::Node(node) => f1.init(node),
                SeedNodeInner::EntryRoot  => heap_fn(),
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
    type N2   = SeedNode<N>;
    type MapH = H;
    type MapR = R;

    fn apply<T>(
        &self,
        treeish: <Local as Domain<N>>::Graph<N>,
        fold:    <Local as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <Local as Domain<SeedNode<N>>>::Graph<SeedNode<N>>,
            <Local as Domain<SeedNode<N>>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        use crate::domain::local::{self, edgy as local_edgy};

        // ── lifted treeish ─────────────────────────────────
        let sl_grow     = self.grow.clone();
        let entry_seeds = self.entry_seeds.clone();
        let base        = treeish;
        let lifted_treeish: local_edgy::Treeish<SeedNode<N>> =
            local_edgy::treeish_visit(
                move |n: &SeedNode<N>, cb: &mut dyn FnMut(&SeedNode<N>)| match &n.inner {
                    SeedNodeInner::Node(node) => {
                        base.visit(node, &mut |c: &N| cb(&SeedNode::node(c.clone())));
                    }
                    SeedNodeInner::EntryRoot => entry_seeds.visit(&(),
                        &mut |s: &Seed| cb(&SeedNode::node(sl_grow(s)))),
                },
            );

        // ── lifted fold ────────────────────────────────────
        let heap_fn = self.entry_heap_fn.clone();
        let f1 = fold.clone(); let f2 = fold.clone(); let f3 = fold;
        let lifted_fold: local::Fold<SeedNode<N>, H, R> = local::fold(
            move |n: &SeedNode<N>| match &n.inner {
                SeedNodeInner::Node(node) => f1.init(node),
                SeedNodeInner::EntryRoot  => heap_fn(),
            },
            move |heap: &mut H, result: &R| f2.accumulate(heap, result),
            move |heap: &H| f3.finalize(heap),
        );

        cont(lifted_treeish, lifted_fold)
    }
}
