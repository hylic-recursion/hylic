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
        grow: impl Fn(&Seed) -> N + Send + Sync + 'static,
        treeish: Treeish<N>,
        seeds_from_top: Edgy<Top, Seed>,
        fold: &shared::fold::Fold<N, H, R>,
        heap_of_top: impl Fn(&Top) -> H + Send + Sync + 'static,
    ) -> Self {
        SeedPipeline {
            seed_lift: SeedLift::new(grow),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::shared::{self as dom, fold};
    use crate::graph;

    // A simple model: Node = usize, Seed = usize.
    // grow is identity (or a transformation).
    // The tree is an adjacency list.

    type N = usize;
    type S = usize;

    fn test_children() -> Vec<Vec<usize>> {
        // 0 → [1, 2], 1 → [3], 2 → [], 3 → []
        vec![vec![1, 2], vec![3], vec![], vec![]]
    }

    fn test_treeish() -> graph::Treeish<N> {
        let ch = test_children();
        graph::treeish_visit(move |n: &N, cb: &mut dyn FnMut(&N)| {
            for &c in &ch[*n] { cb(&c); }
        })
    }

    fn sum_fold() -> fold::Fold<N, u64, u64> {
        fold::fold(
            |n: &N| *n as u64,
            |h: &mut u64, c: &u64| *h += c,
            |h: &u64| *h,
        )
    }

    // ── Core lift mechanics ─────────────────────────

    #[test]
    fn convergence_right_entry() {
        // Entering through Right(node) produces the same result as
        // running the original fold on the original treeish.
        let treeish = test_treeish();
        let f = sum_fold();
        let lift = SeedLift::<N, S>::new(|seed: &S| *seed);

        let original = dom::FUSED.run(&f, &treeish, &0);

        let lt = lift.lift_treeish(treeish);
        let lf = lift.lift_fold(f);
        let lifted = dom::FUSED.run(&lf, &lt, &Either::Right(0));

        assert_eq!(original, lifted);
    }

    #[test]
    fn seed_entry_grows_then_converges() {
        // Left(seed) → grow → node. Result must match running from that node.
        let treeish = test_treeish();
        let f = sum_fold();
        let lift = SeedLift::<N, S>::new(|seed: &S| *seed);

        let direct = dom::FUSED.run(&f, &treeish, &1);

        let lt = lift.lift_treeish(treeish);
        let lf = lift.lift_fold(f);
        let via_seed = dom::FUSED.run(&lf, &lt, &Either::Left(1));

        assert_eq!(direct, via_seed);
    }

    #[test]
    fn relay_passes_leaf_result() {
        // A leaf entered via Left(seed): init returns the node value,
        // no children, finalize returns it. Relay passes through unchanged.
        let treeish = test_treeish();
        let f = sum_fold();
        let lift = SeedLift::<N, S>::new(|seed: &S| *seed);

        let lt = lift.lift_treeish(treeish);
        let lf = lift.lift_fold(f);
        assert_eq!(dom::FUSED.run(&lf, &lt, &Either::Left(3)), 3);
    }

    #[test]
    fn grow_transforms_seed() {
        // grow doubles the seed. Left(1) → node 2 (leaf) → result = 2.
        let treeish = test_treeish();
        let f = sum_fold();
        let lift = SeedLift::<N, S>::new(|seed: &S| seed * 2);

        let lt = lift.lift_treeish(treeish.clone());
        let lf = lift.lift_fold(f.clone());

        let result = dom::FUSED.run(&lf, &lt, &Either::Left(1));
        let direct = dom::FUSED.run(&f, &treeish, &2);
        assert_eq!(result, direct);
    }

    // ── SeedPipeline ────────────────────────────────

    #[test]
    fn pipeline_single_top_seed() {
        type Top = Vec<S>;

        let pipeline = SeedPipeline::<N, S, Top, u64, u64>::new(
            |seed: &S| *seed,
            test_treeish(),
            graph::edgy(|top: &Top| top.clone()),
            &sum_fold(),
            |_top: &Top| 0u64,
        );

        let result = pipeline.run(&dom::FUSED, &vec![0usize]);
        let expected = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);
        assert_eq!(result, expected);
    }

    #[test]
    fn pipeline_multiple_top_seeds() {
        type Top = Vec<S>;

        let pipeline = SeedPipeline::<N, S, Top, u64, u64>::new(
            |seed: &S| *seed,
            test_treeish(),
            graph::edgy(|top: &Top| top.clone()),
            &sum_fold(),
            |_top: &Top| 0u64,
        );

        // Seeds [1, 2]: subtree(1) = 1+3 = 4, subtree(2) = 2. Total = 6.
        let result = pipeline.run(&dom::FUSED, &vec![1usize, 2]);
        assert_eq!(result, 6);
    }

    #[test]
    fn pipeline_run_node_matches_direct() {
        type Top = Vec<S>;

        let pipeline = SeedPipeline::<N, S, Top, u64, u64>::new(
            |seed: &S| *seed,
            test_treeish(),
            graph::edgy(|top: &Top| top.clone()),
            &sum_fold(),
            |_top: &Top| 0u64,
        );

        let result = pipeline.run_node(&dom::FUSED, &0);
        let expected = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);
        assert_eq!(result, expected);
    }

    // ── LiftOps trait interop ───────────────────────

    #[test]
    fn liftops_via_run_lifted() {
        // SeedLift implements LiftOps — verify through the free function.
        let treeish = test_treeish();
        let f = sum_fold();
        let lift = SeedLift::<N, S>::new(|seed: &S| *seed);

        let original = dom::FUSED.run(&f, &treeish, &0);
        let result = crate::cata::lift::run_lifted(&dom::FUSED, &lift, &f, &treeish, &0);
        assert_eq!(original, result);
    }

    // ── Funnel executor ─────────────────────────────

    #[test]
    fn pipeline_with_funnel() {
        use crate::cata::exec::funnel;

        type Top = Vec<S>;

        let pipeline = SeedPipeline::<N, S, Top, u64, u64>::new(
            |seed: &S| *seed,
            test_treeish(),
            graph::edgy(|top: &Top| top.clone()),
            &sum_fold(),
            |_top: &Top| 0u64,
        );

        let expected = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);

        // One-shot funnel execution
        let result = pipeline.run(
            &dom::exec(funnel::Spec::default(2)),
            &vec![0usize],
        );
        assert_eq!(result, expected, "funnel one-shot must match fused");

        // Session-scoped funnel execution
        dom::exec(funnel::Spec::default(2)).session(|s| {
            let result = pipeline.run(s.inner(), &vec![0usize]);
            assert_eq!(result, expected, "funnel session must match fused");
        });
    }

    // ── Error domain (Either<Err, Valid>) ───────────

    #[test]
    fn error_nodes_are_leaves() {
        // Models mb_resolver's Node = Either<Error, ValidModule>.
        // Error nodes have no children (seeds_from_node yields nothing).
        // The lift must handle this: grow produces an error node,
        // the treeish yields no children, the fold inits and finalizes.

        #[derive(Clone, Debug, PartialEq)]
        enum ResNode {
            Ok(u64, Vec<u64>),    // value + child indices
            Err(String),           // error leaf
        }

        type Seed = u64;

        let nodes: Vec<ResNode> = vec![
            ResNode::Ok(10, vec![1, 2]),  // 0: ok, children [1, 2]
            ResNode::Ok(20, vec![3]),      // 1: ok, child [3]
            ResNode::Err("bad".into()),    // 2: error leaf
            ResNode::Ok(30, vec![]),       // 3: ok leaf
        ];

        let nodes_for_treeish = nodes.clone();
        let treeish = graph::treeish_visit(move |n: &ResNode, cb: &mut dyn FnMut(&ResNode)| {
            match n {
                ResNode::Ok(_, children) => {
                    for &idx in children {
                        cb(&nodes_for_treeish[idx as usize]);
                    }
                }
                ResNode::Err(_) => {} // error nodes have no children
            }
        });

        let f = fold::fold(
            |n: &ResNode| match n {
                ResNode::Ok(v, _) => *v,
                ResNode::Err(_) => 0,
            },
            |h: &mut u64, c: &u64| *h += c,
            |h: &u64| *h,
        );

        let nodes_for_grow = nodes.clone();
        let lift = SeedLift::<ResNode, Seed>::new(move |seed: &Seed| {
            nodes_for_grow[*seed as usize].clone()
        });

        // Direct execution from node 0: 10 + (20 + 30) + 0 = 60
        let direct = dom::FUSED.run(&f, &treeish, &nodes[0]);
        assert_eq!(direct, 60);

        // Via seed lift: Left(0) → grow → nodes[0], then same tree
        let lt = lift.lift_treeish(treeish);
        let lf = lift.lift_fold(f);
        let via_seed = dom::FUSED.run(&lf, &lt, &Either::Left(0));
        assert_eq!(via_seed, direct);

        // Error node via seed: Left(2) → grow → Err("bad") → leaf → 0
        let error_result = dom::FUSED.run(&lf, &lt, &Either::Left(2));
        assert_eq!(error_result, 0, "error node must be a leaf with init value");
    }
}
