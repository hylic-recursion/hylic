//! SeedLift: anamorphic lift for seed-based graph construction.
//!
//! The lift embeds the original treeish inside a LiftedNode tree with
//! three variants: Entry (root branching), Seed (single-child relay),
//! Node (original fold + treeish).
//!
//! SeedPipeline stores grow, seeds_from_node, and fold decomposed.
//! Fusion into the lifted graph happens at the point of use (inside
//! with_lifted CPS). This preserves independent transformability.
//!
//! Bounds are minimal: the pipeline struct and transforms require only
//! 'static. Executor-specific bounds (Send, Sync, Clone) appear on
//! run methods where they're actually needed.

use std::sync::Arc;
use crate::domain::{self, shared};
use crate::graph::{self, Edgy, Treeish};
use super::exec::Executor;

// ── LiftedNode: the three-variant lifted node ──────

#[derive(Clone)]
pub enum LiftedNode<Seed, N> {
    Entry,
    Seed(Seed),
    Node(N),
}

// ── LiftedHeap: the lifted heap ────────────────────

// ANCHOR: seed_heap
pub enum LiftedHeap<H, R> {
    Active(H),
    Relay(Option<R>),
}
// ANCHOR_END: seed_heap

impl<H: Clone, R: Clone> Clone for LiftedHeap<H, R> {
    fn clone(&self) -> Self {
        match self {
            LiftedHeap::Active(h) => LiftedHeap::Active(h.clone()),
            LiftedHeap::Relay(r) => LiftedHeap::Relay(r.clone()),
        }
    }
}

// ── SeedLift: the FP core ──────────────────────────

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

    // ANCHOR: lift_treeish
    pub fn lift_treeish(
        &self,
        t: Treeish<N>,
        entry_seeds: Edgy<(), Seed>,
    ) -> Treeish<LiftedNode<Seed, N>> {
        let grow = self.grow.clone();
        graph::treeish_visit(move |n: &LiftedNode<Seed, N>, cb: &mut dyn FnMut(&LiftedNode<Seed, N>)| {
            match n {
                LiftedNode::Node(node) => {
                    t.visit(node, &mut |child: &N| {
                        cb(&LiftedNode::Node(child.clone()));
                    });
                }
                LiftedNode::Seed(seed) => {
                    cb(&LiftedNode::Node(grow(seed)));
                }
                LiftedNode::Entry => {
                    entry_seeds.visit(&(), &mut |seed: &Seed| {
                        cb(&LiftedNode::Node(grow(seed)));
                    });
                }
            }
        })
    }
    // ANCHOR_END: lift_treeish

    /// Lift fold. entry_heap_fn is a factory: produces the entry
    /// node's initial heap on demand. H itself needs no bounds —
    /// the factory closure carries Send + Sync.
    pub fn lift_fold<H: 'static, R: Clone + 'static>(
        &self,
        f: shared::fold::Fold<N, H, R>,
        entry_heap_fn: impl Fn() -> H + Send + Sync + 'static,
    ) -> shared::fold::Fold<LiftedNode<Seed, N>, LiftedHeap<H, R>, R> {
        let f1 = f.clone();
        let f2 = f.clone();
        let f3 = f;
        shared::fold::fold(
            move |n: &LiftedNode<Seed, N>| -> LiftedHeap<H, R> {
                match n {
                    LiftedNode::Node(node) => LiftedHeap::Active(f1.init(node)),
                    LiftedNode::Seed(_) => LiftedHeap::Relay(None),
                    LiftedNode::Entry => LiftedHeap::Active(entry_heap_fn()),
                }
            },
            move |heap: &mut LiftedHeap<H, R>, result: &R| {
                match heap {
                    LiftedHeap::Active(h) => f2.accumulate(h, result),
                    LiftedHeap::Relay(slot) => *slot = Some(result.clone()),
                }
            },
            move |heap: &LiftedHeap<H, R>| -> R {
                match heap {
                    LiftedHeap::Active(h) => f3.finalize(h),
                    LiftedHeap::Relay(Some(r)) => r.clone(),
                    LiftedHeap::Relay(None) => panic!("relay finalized without child result"),
                }
            },
        )
    }
}

// ── SeedPipeline: user-facing wrapper ──────────────

/// Stores grow, seeds_from_node, and fold decomposed. Fusion into
/// the lifted graph happens at the point of use. Entry concerns are
/// supplied at run time.
///
/// Minimal bounds: only 'static on type parameters. Executor-specific
/// bounds (Send, Sync, Clone) appear on individual methods.
pub struct SeedPipeline<N, Seed, H, R> {
    grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
    seeds_from_node: Edgy<N, Seed>,
    fold: shared::fold::Fold<N, H, R>,
}

impl<N, Seed, H, R> Clone for SeedPipeline<N, Seed, H, R> {
    fn clone(&self) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.clone(),
        }
    }
}

// ── Construction + transforms (minimal bounds) ──────

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R>
where
    N: 'static,
    Seed: 'static,
    H: 'static,
    R: 'static,
{
    pub fn new(
        grow: impl Fn(&Seed) -> N + Send + Sync + 'static,
        seeds_from_node: Edgy<N, Seed>,
        fold: &shared::fold::Fold<N, H, R>,
    ) -> Self {
        SeedPipeline {
            grow: Arc::new(grow),
            seeds_from_node,
            fold: fold.clone(),
        }
    }

    // ── R-type transforms ───────────────────────────

    pub fn zipmap<Extra: 'static>(
        &self,
        mapper: impl Fn(&R) -> Extra + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, (R, Extra)>
    where R: Clone,
    {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.zipmap(mapper),
        }
    }

    pub fn map<RNew: 'static>(
        &self,
        mapper: impl Fn(&R) -> RNew + Send + Sync + 'static,
        backmapper: impl Fn(&RNew) -> R + Send + Sync + 'static,
    ) -> SeedPipeline<N, Seed, H, RNew> {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.map(mapper, backmapper),
        }
    }

    // ── Constituent transforms ──────────────────────

    pub fn wrap_grow(
        &self,
        wrapper: impl Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + Send + Sync + 'static,
    ) -> Self {
        let old = self.grow.clone();
        SeedPipeline {
            grow: Arc::new(move |s: &Seed| wrapper(s, &|s| old(s))),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.clone(),
        }
    }

    pub fn filter_seeds(
        &self,
        pred: impl Fn(&Seed) -> bool + Send + Sync + 'static,
    ) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.filter(pred),
            fold: self.fold.clone(),
        }
    }

    pub fn wrap_init(
        &self,
        wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    ) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.wrap_init(wrapper),
        }
    }

    pub fn wrap_accumulate(
        &self,
        wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    ) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.wrap_accumulate(wrapper),
        }
    }

    pub fn wrap_finalize(
        &self,
        wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    ) -> Self {
        SeedPipeline {
            grow: self.grow.clone(),
            seeds_from_node: self.seeds_from_node.clone(),
            fold: self.fold.wrap_finalize(wrapper),
        }
    }
}

// ── Internal: late fusion ───────────────────────────

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    H: 'static,
    R: Clone + 'static,
{
    // ANCHOR: treeish_from_seeds
    fn compose_treeish(&self) -> Treeish<N> {
        self.seeds_from_node.map({
            let g = self.grow.clone();
            move |seed: &Seed| g(seed)
        })
    }
    // ANCHOR_END: treeish_from_seeds

    fn make_seed_lift(&self) -> SeedLift<N, Seed> {
        SeedLift { grow: self.grow.clone() }
    }
}

// ── CPS: the single fusion point ────────────────────

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    H: 'static,
    R: Clone + 'static,
{
    /// Compose all parts, lift into the LiftedNode graph, pass the
    /// artifacts to a continuation. entry_heap_fn produces H on
    /// demand — H needs no bounds beyond 'static.
    pub fn with_lifted<T>(
        &self,
        entry_seeds: Edgy<(), Seed>,
        entry_heap_fn: impl Fn() -> H + Send + Sync + 'static,
        cont: impl FnOnce(
            &shared::fold::Fold<LiftedNode<Seed, N>, LiftedHeap<H, R>, R>,
            &Treeish<LiftedNode<Seed, N>>,
        ) -> T,
    ) -> T {
        let treeish = self.compose_treeish();
        let seed_lift = self.make_seed_lift();
        let lifted_fold = seed_lift.lift_fold(self.fold.clone(), entry_heap_fn);
        let lifted_treeish = seed_lift.lift_treeish(treeish, entry_seeds);
        cont(&lifted_fold, &lifted_treeish)
    }
}

// ── Run methods ─────────────────────────────────────
//
// Bounds: N and Seed need Clone + 'static (for late fusion).
// H needs Clone + Send + Sync (for factory closure wrapping).
// R needs Clone + 'static. Executor-specific bounds (N: Send etc)
// propagate from the Executor trait bound on exec, not from here.

impl<N, Seed, H, R> SeedPipeline<N, Seed, H, R>
where
    N: Clone + 'static,
    Seed: Clone + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + 'static,
{
    /// Enter with a streaming edge function that produces seeds.
    pub fn run(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        entry_seeds: Edgy<(), Seed>,
        entry_heap: H,
    ) -> R {
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Entry))
    }

    /// Enter with a slice of seeds.
    pub fn run_from_slice(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        seeds: &[Seed],
        entry_heap: H,
    ) -> R
    where Seed: Send + Sync,
    {
        let owned = seeds.to_vec();
        let entry_seeds = graph::edgy_visit(move |_: &(), cb: &mut dyn FnMut(&Seed)| {
            for s in &owned { cb(s); }
        });
        self.run(exec, entry_seeds, entry_heap)
    }

    /// Enter with a single seed.
    pub fn run_seed(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        seed: &Seed,
        entry_heap: H,
    ) -> R
    where Seed: Send + Sync,
    {
        self.run_from_slice(exec, &[seed.clone()], entry_heap)
    }

    /// Enter with a resolved node.
    pub fn run_node(
        &self,
        exec: &impl Executor<LiftedNode<Seed, N>, R, domain::Shared, Treeish<LiftedNode<Seed, N>>>,
        node: &N,
        entry_heap: H,
    ) -> R {
        let entry_seeds = graph::edgy_visit(|_: &(), _: &mut dyn FnMut(&Seed)| {});
        self.with_lifted(entry_seeds, move || entry_heap.clone(),
            |fold, treeish| exec.run(fold, treeish, &LiftedNode::Node(node.clone())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::shared::{self as dom, fold};
    use crate::graph;
    use std::sync::{Arc, Mutex};

    type N = usize;
    type S = usize;

    fn test_children() -> Vec<Vec<usize>> {
        vec![vec![1, 2], vec![3], vec![], vec![]]
    }

    fn test_seeds_from_node() -> graph::Edgy<N, S> {
        let ch = test_children();
        graph::edgy_visit(move |n: &N, cb: &mut dyn FnMut(&S)| {
            for &c in &ch[*n] { cb(&c); }
        })
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

    fn make_pipeline() -> SeedPipeline<N, S, u64, u64> {
        SeedPipeline::new(|seed: &S| *seed, test_seeds_from_node(), &sum_fold())
    }

    // ── Core lift mechanics ─────────────────────────

    #[test]
    fn convergence_node_entry() {
        let pipeline = make_pipeline();
        let original = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);
        let result = pipeline.run_node(&dom::FUSED, &0, 0u64);
        assert_eq!(original, result);
    }

    #[test]
    fn seed_entry_grows_then_converges() {
        let pipeline = make_pipeline();
        let direct = dom::FUSED.run(&sum_fold(), &test_treeish(), &1);
        let result = pipeline.run_seed(&dom::FUSED, &1, 0u64);
        assert_eq!(direct, result);
    }

    #[test]
    fn entry_branches_into_seeds() {
        let pipeline = make_pipeline();
        let r1 = dom::FUSED.run(&sum_fold(), &test_treeish(), &1);
        let r2 = dom::FUSED.run(&sum_fold(), &test_treeish(), &2);
        let result = pipeline.run_from_slice(&dom::FUSED, &[1, 2], 0u64);
        assert_eq!(result, r1 + r2);
    }

    // ── Pipeline: convenience entry ─────────────────

    #[test]
    fn pipeline_from_slice() {
        let pipeline = make_pipeline();
        let result = pipeline.run_from_slice(&dom::FUSED, &[0], 0u64);
        let expected = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);
        assert_eq!(result, expected);
    }

    #[test]
    fn pipeline_from_slice_multiple_seeds() {
        let pipeline = make_pipeline();
        let result = pipeline.run_from_slice(&dom::FUSED, &[1, 2], 0u64);
        assert_eq!(result, 6);
    }

    // ── Pipeline: custom entry via Edgy ─────────────

    #[test]
    fn pipeline_custom_entry() {
        struct MyTop { roots: Vec<usize> }

        let pipeline = make_pipeline();
        let my_top = MyTop { roots: vec![1, 2] };
        let entry_seeds = graph::edgy_visit({
            let roots = my_top.roots.clone();
            move |_: &(), cb: &mut dyn FnMut(&usize)| {
                for r in &roots { cb(r); }
            }
        });

        let result = pipeline.run(&dom::FUSED, entry_seeds, 0u64);
        assert_eq!(result, 6);
    }

    // ── CPS: with_lifted ────────────────────────────

    #[test]
    fn with_lifted_multiple_runs() {
        let pipeline = make_pipeline();
        let entry = graph::edgy_visit(|_: &(), cb: &mut dyn FnMut(&S)| { cb(&0); });

        pipeline.with_lifted(entry, || 0u64, |fold, treeish| {
            let r1 = dom::FUSED.run(fold, treeish, &LiftedNode::Entry);
            let r2 = dom::FUSED.run(fold, treeish, &LiftedNode::Entry);
            assert_eq!(r1, r2);
            assert_eq!(r1, 6);
        });
    }

    #[test]
    fn with_lifted_node_entry() {
        let pipeline = make_pipeline();
        let entry = graph::edgy_visit(|_: &(), _: &mut dyn FnMut(&S)| {});

        pipeline.with_lifted(entry, || 0u64, |fold, treeish| {
            let r = dom::FUSED.run(fold, treeish, &LiftedNode::Node(0));
            assert_eq!(r, 6);
        });
    }

    // ── Constituent transforms ──────────────────────

    #[test]
    fn wrap_grow_adds_logging() {
        let pipeline = make_pipeline();
        let log: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let transformed = pipeline.wrap_grow({
            let log = log.clone();
            move |seed: &S, original: &dyn Fn(&S) -> N| {
                log.lock().unwrap().push(*seed);
                original(seed)
            }
        });

        transformed.run_from_slice(&dom::FUSED, &[0], 0u64);
        let logged = log.lock().unwrap();
        assert!(logged.contains(&0));
        assert!(logged.contains(&1));
        assert!(logged.contains(&2));
        assert!(logged.contains(&3));
    }

    #[test]
    fn filter_seeds_prunes_dependencies() {
        let pipeline = make_pipeline();
        let pruned = pipeline.filter_seeds(|seed: &S| *seed != 2);
        let result = pruned.run_from_slice(&dom::FUSED, &[0], 0u64);
        assert_eq!(result, 4); // 0+1+3, skipped 2
    }

    #[test]
    fn wrap_finalize_transforms_output() {
        let pipeline = make_pipeline().wrap_finalize(
            |h: &u64, original: &dyn Fn(&u64) -> u64| original(h) * 2
        );
        // Entry: (0+36)*2=72
        let result = pipeline.run_from_slice(&dom::FUSED, &[0], 0u64);
        assert_eq!(result, 72);
    }

    // ── Funnel executor ─────────────────────────────

    #[test]
    fn pipeline_with_funnel() {
        use crate::cata::exec::funnel;

        let pipeline = make_pipeline();
        let expected = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);

        let result = pipeline.run_from_slice(
            &dom::exec(funnel::Spec::default(2)), &[0], 0u64,
        );
        assert_eq!(result, expected, "funnel one-shot must match fused");

        dom::exec(funnel::Spec::default(2)).session(|s| {
            let result = pipeline.run_from_slice(s.inner(), &[0], 0u64);
            assert_eq!(result, expected, "funnel session must match fused");
        });
    }

    // ── Error domain ────────────────────────────────

    #[test]
    fn error_nodes_are_leaves() {
        #[derive(Clone, Debug, PartialEq)]
        enum ResNode {
            Ok(u64, Vec<u64>),
            Err(String),
        }
        type Seed = u64;

        let nodes: Vec<ResNode> = vec![
            ResNode::Ok(10, vec![1, 2]),
            ResNode::Ok(20, vec![3]),
            ResNode::Err("bad".into()),
            ResNode::Ok(30, vec![]),
        ];

        let seeds_from_node = graph::edgy_visit({
            let nodes = nodes.clone();
            move |n: &ResNode, cb: &mut dyn FnMut(&Seed)| {
                if let ResNode::Ok(_, children) = n {
                    for &idx in children { cb(&idx); }
                }
            }
        });

        let f = fold::fold(
            |n: &ResNode| match n { ResNode::Ok(v, _) => *v, ResNode::Err(_) => 0 },
            |h: &mut u64, c: &u64| *h += c,
            |h: &u64| *h,
        );

        let nodes_for_grow = nodes.clone();
        let pipeline = SeedPipeline::new(
            move |seed: &Seed| nodes_for_grow[*seed as usize].clone(),
            seeds_from_node,
            &f,
        );

        let result = pipeline.run_from_slice(&dom::FUSED, &[0], 0u64);
        assert_eq!(result, 60);

        let error_result = pipeline.run_from_slice(&dom::FUSED, &[2], 0u64);
        assert_eq!(error_result, 0);
    }
}
