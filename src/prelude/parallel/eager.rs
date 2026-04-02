//! Pipelined eager parallel fold as a Lift — domain-generic.
//!
//! Phase 1 (fused traversal): runs fold.init per node, builds
//! Completion handles. Leaf finalize submits Phase 2 work immediately.
//! Interior finalize wires a Collector counting down children.
//!
//! Phase 2 (continuation-passing, overlapping with Phase 1):
//! When a child completes, it notifies the parent Collector. The LAST
//! child to arrive runs the parent's acc+fin INLINE — no new task, no
//! blocking. The chain propagates upward to the root.
//!
//! Domain-generic via FoldPtr: a lifetime-erased raw pointer to the
//! fold's operations. The fold lives in the stash (stable heap address)
//! and outlives all tasks (unwrap waits for root before dropping it).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::cata::Lift;
use crate::domain::{ConstructFold, Domain};
use crate::ops::FoldOps;
use super::completion::Completion;
use super::pool::{WorkPool, WorkPoolSpec};
use super::sync_unsafe::FoldPtr;

// ── EagerSpec ────────────────────────────────────────

/// Controls the eager lift's parallelism granularity.
pub struct EagerSpec {
    /// Minimum children to create a Collector. Below this threshold,
    /// children are waited on inline (the worker helps the pool while
    /// waiting). Default: 2.
    pub min_children_to_fork: usize,
    /// Minimum subtree height to fork. Nodes whose tallest child
    /// subtree has height < this threshold go sequential. Height 0 =
    /// leaf; height 1 = parent of leaves only. Default: 2.
    pub min_height_to_fork: usize,
}

impl EagerSpec {
    pub fn default_for(_n_workers: usize) -> Self {
        EagerSpec {
            min_children_to_fork: 2,
            min_height_to_fork: 2,
        }
    }
}

// ── Collector ────────────────────────────────────────

/// Reactive parent computation. Counts down as children complete.
/// The LAST child runs acc+fin INLINE on its thread — no task
/// submission, no blocking.
struct Collector<N: 'static, H: 'static, R: 'static> {
    remaining: AtomicUsize,
    heap: Mutex<H>,
    child_results: Mutex<Vec<Option<R>>>,
    parent_completion: Completion<R>,
    fold: FoldPtr<N, H, R>,
}

impl<N: 'static, H: Send + 'static, R: Clone + Send + 'static> Collector<N, H, R> {
    fn child_done(self: &Arc<Self>, child_index: usize, result: R) {
        {
            self.child_results.lock().unwrap()[child_index] = Some(result);
        }
        let prev = self.remaining.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            // Last child — run acc+fin inline.
            // SAFETY: fold pointer valid — this Collector firing is on the
            // path to root completion. The stash (holding the fold) is not
            // taken until unwrap waits for root.
            let mut h = self.heap.lock().unwrap();
            let results = self.child_results.lock().unwrap();
            for r in results.iter() {
                unsafe { self.fold.accumulate(&mut h, r.as_ref().unwrap()) };
            }
            let result = unsafe { self.fold.finalize(&h) };
            drop(results);
            drop(h);
            self.parent_completion.set(result);
        }
    }
}

// Collector fields: Mutex<H> (Send if H: Send), Mutex<Vec<Option<R>>> (Send if R: Send),
// Completion<R> (Arc-based, Send), FoldPtr (Send+Sync by assertion), AtomicUsize (Send+Sync).
unsafe impl<N, H: Send, R: Send> Send for Collector<N, H, R> {}
unsafe impl<N, H: Send, R: Send> Sync for Collector<N, H, R> {}

// ── Lifted types ─────────────────────────────────────

pub struct EagerHeap<H, R> {
    heap: H,
    children: Vec<Completion<R>>,
    max_child_height: usize,
}

pub struct EagerResult<R> {
    completion: Completion<R>,
    height: usize,
}

impl<R> Clone for EagerResult<R> {
    fn clone(&self) -> Self { EagerResult { completion: self.completion.clone(), height: self.height } }
}

// ── ParEager ─────────────────────────────────────────

/// Eager parallel strategy. Phase 1 and Phase 2 overlap — leaf work
/// starts during the fused traversal. Domain-generic via FoldPtr.
pub struct ParEager;

impl ParEager {
    pub fn lift<D, N, H, R>(
        pool: &Arc<WorkPool>,
        spec: EagerSpec,
    ) -> Lift<D, N, H, R, N, EagerHeap<H, R>, EagerResult<R>>
    where
        D: Domain<N> + ConstructFold<N>,
        <D as Domain<N>>::Fold<H, R>: Clone,
        N: Clone + 'static,
        H: Clone + Send + 'static,
        R: Clone + Send + 'static,
    {
        let pool_for_unwrap = pool.clone();
        let pool_for_fin = pool.clone();

        let stash: Rc<RefCell<Option<<D as Domain<N>>::Fold<H, R>>>> = Rc::new(RefCell::new(None));
        let stash_write = stash.clone();
        let stash_read = stash.clone();

        let min_fork = spec.min_children_to_fork;
        let min_height = spec.min_height_to_fork;

        Lift::new(
            // lift_treeish: identity
            |treeish| treeish,

            // lift_fold: stash fold, create FoldPtr, build Phase 1 fold
            move |original_fold: <D as Domain<N>>::Fold<H, R>| {
                // Clone for the init closure (needs fold.init during Phase 1)
                let fold_for_init = original_fold.clone();
                // Stash for FoldPtr (acc+fin during Phase 2)
                *stash_write.borrow_mut() = Some(original_fold);

                // SAFETY: fold is in the stash at a stable heap address (Rc
                // allocation). Pointer valid until unwrap takes from stash.
                let fold_ptr = unsafe {
                    let stash_ref = stash_write.borrow();
                    FoldPtr::from_ref(stash_ref.as_ref().unwrap())
                };

                let pool = pool_for_fin.clone();

                // SAFETY (make_fold for Shared): init captures fold_for_init
                // (D::Fold, which is Arc-based for Shared → Send+Sync).
                // acc captures nothing domain-specific. fin captures FoldPtr
                // (Send+Sync) + Arc<WorkPool> (Send+Sync) + usize.
                unsafe { D::make_fold(
                    // ── init: call original fold's init ──
                    move |node: &N| -> EagerHeap<H, R> {
                        EagerHeap {
                            heap: fold_for_init.init(node),
                            children: Vec::new(),
                            max_child_height: 0,
                        }
                    },

                    // ── accumulate: collect child Completion handles + track height ──
                    |heap: &mut EagerHeap<H, R>, child: &EagerResult<R>| {
                        heap.children.push(child.completion.clone());
                        if child.height > heap.max_child_height {
                            heap.max_child_height = child.height;
                        }
                    },

                    // ── finalize: wire continuation chain, submit leaves ──
                    move |heap: &EagerHeap<H, R>| -> EagerResult<R> {
                        let completion = Completion::new();
                        let n_children = heap.children.len();
                        let my_height = if n_children == 0 { 0 } else { heap.max_child_height + 1 };
                        let go_sequential = n_children < min_fork
                            || my_height < min_height;

                        if n_children == 0 {
                            // LEAF: submit finalize to pool immediately
                            let h = heap.heap.clone();
                            let fp = fold_ptr;
                            let comp = completion.clone();
                            pool.submit(Box::new(move || {
                                comp.set(fp.finalize(&h));
                            }));
                        } else if go_sequential {
                            // BELOW CUTOFF: wait+help inline, no Collector
                            let mut h = heap.heap.clone();
                            let fp = fold_ptr;
                            for child in &heap.children {
                                let r = child.wait(&pool);
                                fp.accumulate(&mut h, &r);
                            }
                            completion.set(fp.finalize(&h));
                        } else {
                            // PARALLEL: create Collector, wire children
                            let collector = Arc::new(Collector {
                                remaining: AtomicUsize::new(n_children),
                                heap: Mutex::new(heap.heap.clone()),
                                child_results: Mutex::new(
                                    (0..n_children).map(|_| None).collect()
                                ),
                                parent_completion: completion.clone(),
                                fold: fold_ptr,
                            });
                            for (idx, child_comp) in heap.children.iter().enumerate() {
                                let coll = collector.clone();
                                child_comp.attach_parent(Box::new(move |result| {
                                    coll.child_done(idx, result);
                                }));
                            }
                        }

                        EagerResult { completion, height: my_height }
                    },
                ) }
            },

            // lift_root: identity
            |n: &N| n.clone(),

            // unwrap: wait for root, then drop fold from stash
            move |result: EagerResult<R>| {
                let r = result.completion.wait(&pool_for_unwrap);
                // All tasks done — safe to drop fold (invalidates FoldPtrs,
                // but no copies are alive anymore).
                let _fold = stash_read.borrow_mut().take();
                r
            },
        )
    }

    pub fn with<D, N, H, R, Ret>(
        pool_spec: WorkPoolSpec,
        eager_spec: EagerSpec,
        f: impl FnOnce(&Lift<D, N, H, R, N, EagerHeap<H, R>, EagerResult<R>>) -> Ret,
    ) -> Ret
    where
        D: Domain<N> + ConstructFold<N>,
        <D as Domain<N>>::Fold<H, R>: Clone,
        N: Clone + 'static,
        H: Clone + Send + 'static,
        R: Clone + Send + 'static,
    {
        WorkPool::with(pool_spec, |pool| {
            f(&Self::lift(pool, eager_spec))
        })
    }
}
