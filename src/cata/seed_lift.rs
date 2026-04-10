//! SeedLift: anamorphic lift for seed-based graph construction.
//!
//! Expresses the Seed→Node indirection as a LiftOps implementation.
//! The lift embeds the original treeish inside a lifted Either<Seed, N>
//! tree, where Seed nodes are transparent single-child relays that
//! resolve via a `grow` function.
//!
//! After one Seed→Node transition, the original treeish drives all
//! further traversal. The lifted computation converges to the original.

use std::sync::Arc;
use either::Either;
use crate::domain::{self, shared};
use crate::graph::{Treeish, treeish_visit};
use crate::ops::LiftOps;
use super::exec::{Exec, Executor};

// ── SeedHeap: the parallel-world heap ───────────────

/// Heap in the seeded world. Resolved nodes carry the original fold's
/// heap. Seed nodes carry a relay slot for the single child's result.
pub enum SeedHeap<H, R> {
    Node(H),
    Relay(Option<R>),
}

// ── SeedLift: the FP core ───────────────────────────

/// Anamorphic lift: resolves Seed values into Nodes via `grow`.
/// Implements LiftOps — transforms both treeish and fold.
pub struct SeedLift<N, Seed> {
    grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
}

impl<N, Seed> Clone for SeedLift<N, Seed> {
    fn clone(&self) -> Self { SeedLift { grow: self.grow.clone() } }
}

impl<N: Clone + 'static, Seed: Clone + 'static> SeedLift<N, Seed> {
    pub fn new(grow: impl Fn(&Seed) -> N + Send + Sync + 'static) -> Self {
        SeedLift { grow: Arc::new(grow) }
    }

    /// The seeded treeish: embeds the original treeish t.
    /// Right(node) → t.visit(node) → [Right(child)]
    /// Left(seed)  → [Right(grow(seed))]
    pub fn lift_treeish(&self, t: Treeish<N>) -> Treeish<Either<Seed, N>> {
        let grow = self.grow.clone();
        treeish_visit(move |n: &Either<Seed, N>, cb: &mut dyn FnMut(&Either<Seed, N>)| {
            match n {
                Either::Right(node) => {
                    t.visit(node, &mut |child: &N| {
                        let wrapped = Either::Right(child.clone());
                        cb(&wrapped);
                    });
                }
                Either::Left(seed) => {
                    let grown = Either::Right(grow(seed));
                    cb(&grown);
                }
            }
        })
    }

    /// Transform a fold to handle Either<Seed, N>.
    /// Node branch: delegates to the original fold.
    /// Seed branch: transparent relay (stores and returns child R).
    pub fn lift_fold<H: 'static, R: Clone + 'static>(
        &self,
        f: shared::fold::Fold<N, H, R>,
    ) -> shared::fold::Fold<Either<Seed, N>, SeedHeap<H, R>, R> {
        let f1 = f.clone();
        let f2 = f.clone();
        let f3 = f;
        shared::fold::fold(
            move |n: &Either<Seed, N>| -> SeedHeap<H, R> {
                match n {
                    Either::Right(node) => SeedHeap::Node(f1.init(node)),
                    Either::Left(_) => SeedHeap::Relay(None),
                }
            },
            move |heap: &mut SeedHeap<H, R>, result: &R| {
                match heap {
                    SeedHeap::Node(h) => f2.accumulate(h, result),
                    SeedHeap::Relay(slot) => *slot = Some(result.clone()),
                }
            },
            move |heap: &SeedHeap<H, R>| -> R {
                match heap {
                    SeedHeap::Node(h) => f3.finalize(h),
                    SeedHeap::Relay(Some(r)) => r.clone(),
                    SeedHeap::Relay(None) => panic!("seed relay finalized without child result"),
                }
            },
        )
    }
}

impl<N, Seed, R> LiftOps<N, R, Either<Seed, N>>
    for SeedLift<N, Seed>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    R: Clone + 'static,
{
    type LiftedH<H: Clone + 'static> = SeedHeap<H, R>;
    type LiftedR<H: Clone + 'static> = R;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<Either<Seed, N>> {
        SeedLift::lift_treeish(self, t)
    }

    fn lift_fold<H: Clone + 'static>(&self, f: shared::fold::Fold<N, H, R>) -> shared::fold::Fold<Either<Seed, N>, SeedHeap<H, R>, R> {
        SeedLift::lift_fold(self, f)
    }

    fn lift_root(&self, root: &N) -> Either<Seed, N> {
        Either::Right(root.clone())
    }

    fn unwrap<H: Clone + 'static>(&self, result: R) -> R {
        result
    }
}

// ── SeedAdapter: wraps an inner executor ────────────

/// Wraps an inner executor S, lifting at the `.run()` boundary.
/// The user sees original types (N, H, R). Internally, the adapter
/// lifts to Either<Seed, N> and delegates to S.
pub struct SeedAdapter<S, N, Seed> {
    inner: S,
    seed_lift: SeedLift<N, Seed>,
}

impl<S, N, Seed, R> Executor<N, R, domain::Shared, Treeish<N>>
    for SeedAdapter<S, N, Seed>
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + Send + Sync + 'static,
    R: Clone + Send + 'static,
    S: Executor<Either<Seed, N>, R, domain::Shared, Treeish<Either<Seed, N>>>,
{
    fn run<H: 'static>(
        &self,
        fold: &shared::fold::Fold<N, H, R>,
        graph: &Treeish<N>,
        root: &N,
    ) -> R {
        let lifted_fold = self.seed_lift.lift_fold(fold.clone());
        let lifted_treeish = self.seed_lift.lift_treeish(graph.clone());
        self.inner.run(&lifted_fold, &lifted_treeish, &Either::Right(root.clone()))
    }
}

// ── SeedSetup: user-facing factory ──────────────────

/// Carries the seed lift configuration. Produces wrapped executors
/// via `.wrap()`.
pub struct SeedSetup<N, Seed> {
    seed_lift: SeedLift<N, Seed>,
}

impl<N: Clone + 'static, Seed: Clone + 'static> SeedSetup<N, Seed> {
    pub fn new(grow: impl Fn(&Seed) -> N + Send + Sync + 'static) -> Self {
        SeedSetup { seed_lift: SeedLift::new(grow) }
    }

    /// Wrap an executor to produce one that lifts transparently.
    /// The returned Exec operates on the original types (N, H, R).
    pub fn wrap<D, S>(&self, exec: Exec<D, S>) -> Exec<D, SeedAdapter<S, N, Seed>> {
        Exec::new(SeedAdapter {
            inner: exec.into_inner(),
            seed_lift: self.seed_lift.clone(),
        })
    }

    /// Access the underlying SeedLift for direct use.
    pub fn lift(&self) -> &SeedLift<N, Seed> {
        &self.seed_lift
    }
}
