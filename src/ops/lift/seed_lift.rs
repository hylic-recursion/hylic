//! SeedLift — the finishing Lift that retires the Seed axis.
//!
//! `SeedLift<N, Seed, H>` is a `Lift<Shared, N, H, R>` with
//! `N2 = LiftedNode<Seed, N>`, `MapH = LiftedHeap<H, R>`, `MapR = R`.
//! It carries Entry-dispatch state as struct fields:
//!
//!   - `grow`: the pipeline's Seed→N resolver (shared via Arc clone
//!     with the pipeline's own grow).
//!   - `entry_seeds`: `Edgy<(), Seed>` — the canonical
//!     callback-iterator form; convenience wrappers lower `&[Seed]`
//!     into this shape at the user surface.
//!   - `entry_heap_fn`: produces the root heap at Entry.
//!
//! Usage:
//!   - Library-internal: `PipelineExec::run` constructs a SeedLift
//!     from the pipeline's grow + user's entry_seeds/entry_heap and
//!     invokes `sl.apply(...)` on the yielded triple.
//!   - User-explicit: compose directly via `apply_pre_lift(SeedLift::new(...))`.
//!     The chain consumes `.run_from_node(&exec, &LiftedNode::Entry)`.
//!
//! Shared-pinned: the apply body constructs a `shared::fold::Fold`
//! directly. Local / Owned finishing-lift equivalents are separate
//! types; only one domain is covered here by design.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::domain::{Domain, Shared};
use crate::domain::shared::fold::{self as sfold, Fold};
use crate::graph::{self, Edgy, Treeish};
use crate::cata::pipeline::{LiftedNode, LiftedHeap};
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
    /// (`apply_pre_lift(SeedLift::new(...))`).
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
    type N2   = LiftedNode<Seed, N>;
    type MapH = LiftedHeap<H, R>;
    type MapR = R;

    fn apply<Seed_, T>(
        &self,
        _grow_upstream: <Shared as Domain<N>>::Grow<Seed_, N>,
        treeish:        <Shared as Domain<N>>::Graph<N>,
        fold:           <Shared as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <Shared as Domain<LiftedNode<Seed, N>>>::Grow<Seed_, LiftedNode<Seed, N>>,
            <Shared as Domain<LiftedNode<Seed, N>>>::Graph<LiftedNode<Seed, N>>,
            <Shared as Domain<LiftedNode<Seed, N>>>::Fold<LiftedHeap<H, R>, R>,
        ) -> T,
    ) -> T
    where Seed_: Clone + 'static,
    {
        // ── lifted treeish ─────────────────────────────────
        let sl_grow      = self.grow.clone();
        let entry_seeds  = self.entry_seeds.clone();
        let base_treeish = treeish;
        let lifted_treeish: Treeish<LiftedNode<Seed, N>> = graph::treeish_visit(
            move |n: &LiftedNode<Seed, N>, cb: &mut dyn FnMut(&LiftedNode<Seed, N>)| {
                match n {
                    LiftedNode::Node(node) => {
                        base_treeish.visit(node,
                            &mut |c: &N| cb(&LiftedNode::Node(c.clone())));
                    }
                    LiftedNode::Seed(s) => cb(&LiftedNode::Node(sl_grow(s))),
                    LiftedNode::Entry   => entry_seeds.visit(&(),
                        &mut |s: &Seed| cb(&LiftedNode::Node(sl_grow(s)))),
                }
            },
        );

        // ── lifted fold ────────────────────────────────────
        let heap_fn = self.entry_heap_fn.clone();
        let f1 = fold.clone(); let f2 = fold.clone(); let f3 = fold;
        let lifted_fold: Fold<LiftedNode<Seed, N>, LiftedHeap<H, R>, R> = sfold::fold(
            move |n: &LiftedNode<Seed, N>| -> LiftedHeap<H, R> {
                match n {
                    LiftedNode::Node(node) => LiftedHeap::Active(f1.init(node)),
                    LiftedNode::Seed(_)    => LiftedHeap::Relay(None),
                    LiftedNode::Entry      => LiftedHeap::Active(heap_fn()),
                }
            },
            move |heap: &mut LiftedHeap<H, R>, result: &R| {
                match heap {
                    LiftedHeap::Active(h)   => f2.accumulate(h, result),
                    LiftedHeap::Relay(slot) => *slot = Some(result.clone()),
                }
            },
            move |heap: &LiftedHeap<H, R>| -> R {
                match heap {
                    LiftedHeap::Active(h)      => f3.finalize(h),
                    LiftedHeap::Relay(Some(r)) => r.clone(),
                    LiftedHeap::Relay(None)    => panic!("relay finalized without child result"),
                }
            },
        );

        // ── unreachable upstream grow ──────────────────────
        let unreachable_grow: Arc<dyn Fn(&Seed_) -> LiftedNode<Seed, N> + Send + Sync> =
            Arc::new(|_: &Seed_| unreachable!(
                "SeedLift is a finishing lift; its output grow is unreachable — \
                 exec.run runs with &LiftedNode::Entry as root"));

        cont(unreachable_grow, lifted_treeish, lifted_fold)
    }
}
