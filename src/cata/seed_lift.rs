//! SeedLift: anamorphic lift for seed-based graph construction.
//!
//! Expresses the Seed→Node indirection as a LiftOps implementation.
//! The lift embeds the original treeish inside a lifted Either<Seed, N>
//! tree, where Seed nodes are transparent single-child relays that
//! resolve via a `grow` function.
//!
//! SeedPipeline bundles the lift with a treeish, fold, top entry
//! mapping, and heap initializer — encapsulating Either<Seed, N>
//! entirely. The user sees only N, Top, H, R.

use std::sync::Arc;
use either::Either;
use crate::domain::{self, shared};
use crate::graph::{Edgy, Treeish, treeish_visit};
use crate::ops::LiftOps;
use super::exec::Executor;

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
    pub fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
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

// ── SeedPipeline: user-facing wrapper ───────────────

/// Bundles a SeedLift with a treeish, fold, top entry mapping, and
/// heap initializer. Encapsulates Either<Seed, N> entirely — the user
/// provides N, Top, H, R and an executor; the internal types are inferred.
pub struct SeedPipeline<N, Seed, Top, H, R> {
    seed_lift: SeedLift<N, Seed>,
    treeish: Treeish<N>,
    seeds_from_top: Edgy<Top, Seed>,
    fold: shared::fold::Fold<N, H, R>,
    heap_of_top: Arc<dyn Fn(&Top) -> H + Send + Sync>,
}

impl<N, Seed, Top, H, R> SeedPipeline<N, Seed, Top, H, R>
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + Send + Sync + 'static,
    Top: 'static,
    H: Clone + 'static,
    R: Clone + Send + 'static,
{
    pub fn new(
        seed_lift: SeedLift<N, Seed>,
        treeish: Treeish<N>,
        seeds_from_top: Edgy<Top, Seed>,
        fold: &shared::fold::Fold<N, H, R>,
        heap_of_top: impl Fn(&Top) -> H + Send + Sync + 'static,
    ) -> Self {
        SeedPipeline {
            seed_lift,
            treeish,
            seeds_from_top,
            fold: fold.clone(),
            heap_of_top: Arc::new(heap_of_top),
        }
    }

    /// Execute the pipeline over a Top entry point.
    /// The executor operates on the internal Either<Seed, N> — inferred,
    /// never named by the caller.
    pub fn run(
        &self,
        exec: &impl Executor<Either<Seed, N>, R, domain::Shared, Treeish<Either<Seed, N>>>,
        top: &Top,
    ) -> R {
        let lifted_fold = self.seed_lift.lift_fold(self.fold.clone());
        let lifted_treeish = self.seed_lift.lift_treeish(self.treeish.clone());

        let mut heap = (self.heap_of_top)(top);
        self.seeds_from_top.visit(top, &mut |seed: &Seed| {
            let root = Either::Left(seed.clone());
            let result = exec.run(&lifted_fold, &lifted_treeish, &root);
            self.fold.accumulate(&mut heap, &result);
        });
        self.fold.finalize(&heap)
    }

    /// Execute on a single root node (no Top, no seeds_from_top).
    /// Enters through Right(node) — the lift is transparent, the original
    /// treeish drives traversal immediately.
    pub fn run_node(
        &self,
        exec: &impl Executor<Either<Seed, N>, R, domain::Shared, Treeish<Either<Seed, N>>>,
        node: &N,
    ) -> R {
        let lifted_fold = self.seed_lift.lift_fold(self.fold.clone());
        let lifted_treeish = self.seed_lift.lift_treeish(self.treeish.clone());
        exec.run(&lifted_fold, &lifted_treeish, &Either::Right(node.clone()))
    }
}
