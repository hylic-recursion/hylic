//! SeedLift — the finishing Lift that retires the Seed axis.
//!
//! `SeedLift<N, Seed, H>` is a `Lift<Shared, N, H, R>` with
//! `N2 = LiftedNode<N>`, `MapH = H`, `MapR = R`. It carries
//! Entry-dispatch state as struct fields:
//!
//!   - `grow`: the pipeline's Seed→N resolver.
//!   - `entry_seeds`: `Edgy<(), Seed>` — canonical callback-iterator.
//!     Convenience wrappers lower `&[Seed]` into this shape.
//!   - `entry_heap_fn`: produces the root heap at Entry.
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
//! Shared-pinned: the apply body constructs a
//! `shared::fold::Fold` directly.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::domain::{Domain, Shared};
use crate::domain::shared::fold::{self as sfold, Fold};
use crate::graph::{self, Edgy, Treeish};
use crate::cata::pipeline::LiftedNode;
use super::core::Lift;

pub struct SeedLift<N, Seed, H>
where N: 'static, Seed: 'static, H: 'static,
{
    pub(crate) grow:          Arc<dyn Fn(&Seed) -> N + Send + Sync>,
    pub(crate) entry_seeds:   Edgy<(), Seed>,
    pub(crate) entry_heap_fn: Arc<dyn Fn() -> H + Send + Sync>,
    _m: PhantomData<fn() -> (N, Seed, H)>,
}

impl<N, Seed, H> Clone for SeedLift<N, Seed, H>
where N: 'static, Seed: 'static, H: 'static,
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

impl<N, Seed, H> SeedLift<N, Seed, H>
where N: 'static, Seed: 'static, H: 'static,
{
    /// Public constructor for user-explicit composition
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
            entry_heap_fn: Arc::new(entry_heap_fn),
            _m: PhantomData,
        }
    }

    /// Crate-internal constructor: reuse a pipeline's pre-built
    /// `Arc<Fn>` grow closure without re-wrapping.
    pub(crate) fn from_arc_grow<HFn>(
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        entry_seeds: Edgy<(), Seed>,
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

impl<N, Seed, H, R> Lift<Shared, N, H, R> for SeedLift<N, Seed, H>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2   = LiftedNode<N>;
    type MapH = H;
    type MapR = R;

    fn apply<Seed_, T>(
        &self,
        _grow_upstream: <Shared as Domain<N>>::Grow<Seed_, N>,
        treeish:        <Shared as Domain<N>>::Graph<N>,
        fold:           <Shared as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <Shared as Domain<LiftedNode<N>>>::Grow<Seed_, LiftedNode<N>>,
            <Shared as Domain<LiftedNode<N>>>::Graph<LiftedNode<N>>,
            <Shared as Domain<LiftedNode<N>>>::Fold<H, R>,
        ) -> T,
    ) -> T
    where Seed_: Clone + 'static,
    {
        // ── lifted treeish ─────────────────────────────────
        let sl_grow     = self.grow.clone();
        let entry_seeds = self.entry_seeds.clone();
        let base        = treeish;
        let lifted_treeish: Treeish<LiftedNode<N>> = graph::treeish_visit(
            move |n: &LiftedNode<N>, cb: &mut dyn FnMut(&LiftedNode<N>)| match n {
                LiftedNode::Node(node) => {
                    base.visit(node, &mut |c: &N| cb(&LiftedNode::Node(c.clone())));
                }
                LiftedNode::Entry => entry_seeds.visit(&(),
                    &mut |s: &Seed| cb(&LiftedNode::Node(sl_grow(s)))),
            },
        );

        // ── lifted fold ────────────────────────────────────
        let heap_fn = self.entry_heap_fn.clone();
        let f1 = fold.clone(); let f2 = fold.clone(); let f3 = fold;
        let lifted_fold: Fold<LiftedNode<N>, H, R> = sfold::fold(
            move |n: &LiftedNode<N>| match n {
                LiftedNode::Node(node) => f1.init(node),
                LiftedNode::Entry      => heap_fn(),
            },
            move |heap: &mut H, result: &R| f2.accumulate(heap, result),
            move |heap: &H| f3.finalize(heap),
        );

        // ── unreachable upstream grow ──────────────────────
        let unreachable_grow: Arc<dyn Fn(&Seed_) -> LiftedNode<N> + Send + Sync> =
            Arc::new(|_: &Seed_| unreachable!(
                "SeedLift is a finishing lift; its output grow is unreachable — \
                 exec.run runs with &LiftedNode::Entry as root"));

        cont(unreachable_grow, lifted_treeish, lifted_fold)
    }
}
