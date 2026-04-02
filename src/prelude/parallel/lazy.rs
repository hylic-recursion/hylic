//! Lazy parallel fold as a Lift — domain-generic.
//!
//! Phase 1 (sequential): builds a data tree of LazyNode values.
//! Phase 2 (parallel): evaluates bottom-up via fork_join_map,
//! borrowing the fold through SyncRef.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use crate::cata::Lift;
use crate::domain::{ConstructFold, Domain};
use crate::ops::FoldOps;
use super::pool::{WorkPool, fork_join_map};
use super::sync_unsafe::SyncRef;

// ── Lazy tree data types ─────────────────────────────

/// Node in the lazy computation tree. Pure data — no fold closures.
struct LazyNode<H, R> {
    heap: H,
    children: Vec<Arc<LazyNode<H, R>>>,
    result: OnceLock<R>,
}

/// Heap during Phase 1 construction.
pub struct LazyHeap<H, R> {
    heap: H,
    children: Vec<Arc<LazyNode<H, R>>>,
}

/// Result handle from Phase 1. Wraps a node in the lazy tree.
pub struct LazyResult<H, R> {
    node: Arc<LazyNode<H, R>>,
}

impl<H, R> Clone for LazyResult<H, R> {
    fn clone(&self) -> Self { LazyResult { node: self.node.clone() } }
}

// ── Phase 2: parallel bottom-up evaluation ───────────

/// Evaluate a single node, recursing into children in parallel.
/// N is a phantom — FoldOps<N,H,R> carries it but eval never calls init.
fn eval_node<N, H: Clone, R: Clone + Send, F: FoldOps<N, H, R>>(
    node: &LazyNode<H, R>,
    fold: &SyncRef<'_, F>,
    pool: &WorkPool,
) -> R {
    node.result.get_or_init(|| {
        let mut h = node.heap.clone();
        let n_children = node.children.len();
        if n_children == 0 {
            fold.finalize(&h)
        } else if n_children == 1 {
            let r = eval_node(&node.children[0], fold, pool);
            fold.accumulate(&mut h, &r);
            fold.finalize(&h)
        } else {
            let results = fork_join_map(
                pool,
                &node.children,
                &|child: &Arc<LazyNode<H, R>>| eval_node(child, fold, pool),
                0, 8,
            );
            for r in &results { fold.accumulate(&mut h, r); }
            fold.finalize(&h)
        }
    }).clone()
}

// ── ParLazy ──────────────────────────────────────────

/// Lazy parallel strategy. Builds a data tree during Phase 1;
/// evaluates bottom-up in parallel during Phase 2. Domain-generic.
pub struct ParLazy;

impl ParLazy {
    pub fn lift<D, N, H, R>(pool: &Arc<WorkPool>) -> Lift<D, N, H, R, N, LazyHeap<H, R>, LazyResult<H, R>>
    where
        D: Domain<N> + ConstructFold<N>,
        <D as Domain<N>>::Fold<H, R>: Clone,
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + Send + 'static,
    {
        let pool = pool.clone();

        // Stash: lift_fold stores the original fold, unwrap retrieves it.
        // Both closures run on the same thread (sequentially), so Rc<RefCell> is safe.
        let stash: Rc<RefCell<Option<<D as Domain<N>>::Fold<H, R>>>> = Rc::new(RefCell::new(None));
        let stash_write = stash.clone();
        let stash_read = stash.clone();

        Lift::new(
            // lift_treeish: identity (node type unchanged)
            |treeish| treeish,

            // lift_fold: stash original, build Phase 1 fold via ConstructFold
            move |original_fold: <D as Domain<N>>::Fold<H, R>| {
                let for_stash = original_fold.clone();
                *stash_write.borrow_mut() = Some(for_stash);
                let f = original_fold;
                // SAFETY: init captures D::Fold (Send+Sync for Shared,
                // unconstrained for Local). acc/fin have no captures.
                unsafe { D::make_fold(
                    move |node: &N| -> LazyHeap<H, R> {
                        LazyHeap { heap: f.init(node), children: Vec::new() }
                    },
                    |heap: &mut LazyHeap<H, R>, child: &LazyResult<H, R>| {
                        heap.children.push(child.node.clone());
                    },
                    |heap: &LazyHeap<H, R>| -> LazyResult<H, R> {
                        LazyResult {
                            node: Arc::new(LazyNode {
                                heap: heap.heap.clone(),
                                children: heap.children.clone(),
                                result: OnceLock::new(),
                            }),
                        }
                    },
                ) }
            },

            // lift_root: identity
            |n: &N| n.clone(),

            // unwrap: retrieve fold from stash, run Phase 2
            move |result: LazyResult<H, R>| {
                let fold = stash_read.borrow_mut().take()
                    .expect("ParLazy: fold not stashed (lift_fold not called?)");
                let sync_fold = SyncRef(&fold);
                eval_node::<N, H, R, _>(&result.node, &sync_fold, &pool)
            },
        )
    }
}
