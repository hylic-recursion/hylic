//! Explainer data types: `ExplainerStep`, `ExplainerHeap`,
//! `ExplainerResult`, and the `treeish_for_explres` navigator.
//!
//! The `Explainer` *lift* is a constructor on the capable domain:
//! call `Shared::explainer_lift()` or `Local::explainer_lift()`,
//! which produces a `ShapeLift<…>` with the trace-building
//! fold-xform. See `technical-insights/09-unified-shape-lift.md`.
//!
//! Types are parametric over `ChildR` to accommodate both the
//! whole-tree `Explainer` (ChildR = ExplainerResult) and
//! `ExplainerDescribe` (ChildR = R, streaming emit).

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use crate::graph::{treeish, Treeish};
use crate::ops::SeedNode;

#[derive(Clone)]
pub struct ExplainerStep<H, ChildR>
where H: Clone, ChildR: Clone,
{
    pub incoming_result: ChildR,
    pub resulting_heap:  H,
}

#[derive(Clone)]
pub struct ExplainerHeap<N, H, ChildR>
where N: Clone, H: Clone, ChildR: Clone,
{
    pub initial_heap: H,
    pub node:         N,
    pub transitions:  Vec<ExplainerStep<H, ChildR>>,
    pub working_heap: H,
}

impl<N: Clone, H: Clone, ChildR: Clone> ExplainerHeap<N, H, ChildR> {
    pub fn new(node: N, heap: H) -> Self {
        ExplainerHeap {
            initial_heap: heap.clone(),
            node,
            transitions:  Vec::new(),
            working_heap: heap,
        }
    }
}

/// Whole-tree trace result. Child traces nest recursively via
/// `heap.transitions[i].incoming_result`.
#[derive(Clone)]
pub struct ExplainerResult<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub orig_result: R,
    pub heap:        ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
}

/// Treeish over `ExplainerResult<N, H, R>` — children are the
/// recursive `incoming_result` of each transition. Useful for
/// running a downstream fold over the captured trace.
pub fn treeish_for_explres<N, H, R>() -> Treeish<ExplainerResult<N, H, R>>
where N: Clone + Send + Sync + 'static,
      H: Clone + Send + Sync + 'static,
      R: Clone + Send + Sync + 'static,
{
    treeish(|x: &ExplainerResult<N, H, R>| {
        x.heap.transitions.iter().map(|step| step.incoming_result.clone()).collect()
    })
}

// ── Seed-closed projection ───────────────────────────────────
//
// On the SeedPipeline path, a chain's tip R after `.explain()` is
// `ExplainerResult<SeedNode<N>, H, R>`: the top row's
// `heap.node` is `SeedNode::Entry` (synthetic), and every nested
// trace carries `SeedNode::Node(n)`. Users who want an N-typed
// view call `SeedExplainerResult::from_lifted`, which splits the
// entry row out as its own fields and recursively projects each
// subtree to `ExplainerResult<N, H, R>`.

/// N-typed projection of a seed-closed explainer result. The Entry
/// row is promoted out of the tree as three fields
/// (`entry_initial_heap`, `entry_working_heap`, `orig_result`) and
/// each root subtree becomes an `ExplainerResult<N, H, R>` —
/// `SeedNode<N>` no longer appears in the user-visible shape.
///
/// Obtain via [`Self::from_lifted`].
#[derive(Clone)]
pub struct SeedExplainerResult<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    /// `entry_heap` argument passed to `.run(...)`, captured before
    /// any root subtree accumulated.
    pub entry_initial_heap: H,
    /// Top-level heap after every root subtree's `R` was accumulated.
    pub entry_working_heap: H,
    /// The chain-tip `R` produced at Entry's finalize.
    pub orig_result:        R,
    /// Per-root-seed subtree traces, in entry-seed order. Each is an
    /// `ExplainerResult` over plain `N`.
    pub roots:              Vec<ExplainerResult<N, H, R>>,
}

impl<N, H, R> SeedExplainerResult<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    /// Project a raw `ExplainerResult<SeedNode<N>, H, R>` (the
    /// chain-tip shape returned by `LiftedSeedPipeline::…run()` after
    /// `.explain()`) into the N-typed form.
    ///
    /// Below the Entry row, every `SeedNode<N>` is known to be a
    /// resolved Node: `SeedNode::Entry` is unique to the root.
    /// This projection walks the trace tree once, unwrapping each
    /// `heap.node` via `SeedNode::as_node`; under the no-Entry-below-root
    /// invariant the `expect` is unreachable.
    pub fn from_lifted(
        raw: ExplainerResult<SeedNode<N>, H, R>,
    ) -> Self {
        // Root's heap.node is SeedNode::Entry. Its transitions are
        // one per root seed; each transition's incoming_result is an
        // ExplainerResult<SeedNode<N>, H, R> whose heap.node is
        // SeedNode::Node(n) — we unwrap recursively.
        let roots: Vec<ExplainerResult<N, H, R>> = raw.heap.transitions
            .into_iter()
            .map(|step| unwrap_lifted_below_root(step.incoming_result))
            .collect();
        SeedExplainerResult {
            entry_initial_heap: raw.heap.initial_heap,
            entry_working_heap: raw.heap.working_heap,
            orig_result:        raw.orig_result,
            roots,
        }
    }
}

/// Recursive unwrap: below Entry, every `SeedNode<N>` must be a
/// `Node(n)`. Panics if the invariant is violated (meaning a lift
/// below `SeedLift` fabricated an Entry row — library bug).
fn unwrap_lifted_below_root<N, H, R>(
    x: ExplainerResult<SeedNode<N>, H, R>,
) -> ExplainerResult<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    let node_lifted = x.heap.node;
    let n: N = node_lifted.into_node()
        .expect("SeedExplainerResult invariant: SeedNode::Entry only at trace root");
    let transitions: Vec<ExplainerStep<H, ExplainerResult<N, H, R>>> = x.heap.transitions
        .into_iter()
        .map(|step| ExplainerStep {
            resulting_heap:  step.resulting_heap,
            incoming_result: unwrap_lifted_below_root(step.incoming_result),
        })
        .collect();
    ExplainerResult {
        orig_result: x.orig_result,
        heap: ExplainerHeap {
            initial_heap: x.heap.initial_heap,
            node:         n,
            transitions,
            working_heap: x.heap.working_heap,
        },
    }
}
