//! Pipelined eager parallel fold as a Lift — domain-generic.
//!
//! Phase 1: fused traversal builds Completion handles. Leaf finalize
//! submits Phase 2 work via ViewHandle. Interior finalize wires Collectors.
//!
//! Phase 2: continuation-passing, overlapping with Phase 1. Last child
//! to arrive runs parent's acc+fin INLINE.
//!
//! The PoolExecView is created in lift_fold (before Phase 1), stored in
//! an Rc stash. ViewHandle (Arc-based, no raw pointers) is injected into
//! finalize closures via ContextSlot. Unwrap waits for root, then drops.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::cata::Lift;
use crate::domain::{ConstructFold, Domain};
use crate::ops::FoldOps;
use super::completion::Completion;
use super::context_slot::ContextSlot;
use super::pool::{WorkPool, WorkPoolSpec, PoolExecView, ViewHandle};
use super::sync_unsafe::FoldPtr;

// ── EagerSpec ────────────────────────────────────────

pub struct EagerSpec {
    pub min_children_to_fork: usize,
    pub min_height_to_fork: usize,
}

impl EagerSpec {
    pub fn default_for(_n_workers: usize) -> Self {
        EagerSpec { min_children_to_fork: 2, min_height_to_fork: 2 }
    }
}

// ── Collector ────────────────────────────────────────

struct Collector<N: 'static, H: 'static, R: 'static> {
    remaining: AtomicUsize,
    heap: Mutex<H>,
    child_results: Mutex<Vec<Option<R>>>,
    parent_completion: Completion<R>,
    fold: FoldPtr<N, H, R>,
}

impl<N: 'static, H: Send + 'static, R: Clone + Send + 'static> Collector<N, H, R> {
    fn child_done(self: &Arc<Self>, child_index: usize, result: R) {
        { self.child_results.lock().unwrap()[child_index] = Some(result); }
        if self.remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
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
    fn clone(&self) -> Self {
        EagerResult { completion: self.completion.clone(), height: self.height }
    }
}

// ── ParEager ─────────────────────────────────────────

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
        let pool_for_lift = pool.clone();

        // Fold stash: lift_fold stores, unwrap retrieves.
        let stash: Rc<RefCell<Option<<D as Domain<N>>::Fold<H, R>>>> =
            Rc::new(RefCell::new(None));
        let stash_write = stash.clone();
        let stash_read = stash.clone();

        // View stash: lift_fold creates the view, unwrap drops it.
        let view_stash: Rc<RefCell<Option<PoolExecView>>> =
            Rc::new(RefCell::new(None));
        let view_stash_write = view_stash.clone();
        let view_stash_read = view_stash.clone();

        // ContextSlot for ViewHandle — filled in lift_fold, read by
        // finalize closures, cleared in unwrap.
        let view_slot: Arc<ContextSlot<ViewHandle>> = Arc::new(ContextSlot::new());
        let view_slot_fill = view_slot.clone();
        let view_slot_clear = view_slot.clone();

        let min_fork = spec.min_children_to_fork;
        let min_height = spec.min_height_to_fork;

        Lift::new(
            |treeish| treeish,

            // lift_fold: stash fold + create view + fill slot + build Phase 1 fold
            move |original_fold: <D as Domain<N>>::Fold<H, R>| {
                let fold_for_init = original_fold.clone();
                *stash_write.borrow_mut() = Some(original_fold);

                let fold_ptr = unsafe {
                    let stash_ref = stash_write.borrow();
                    FoldPtr::from_ref(stash_ref.as_ref().unwrap())
                };

                // Create view and stash it. The view lives in the Rc
                // (heap-stable) until unwrap drops it.
                let view = PoolExecView::new(&pool_for_lift);
                let vh = view.handle(); // Arc-based — stable regardless of moves
                *view_stash_write.borrow_mut() = Some(view);

                // Fill the ContextSlot. Cleared in unwrap.
                unsafe { *view_slot_fill.inner_raw() = Some(vh); }

                let view_slot = view_slot_fill.clone();

                unsafe { D::make_fold(
                    move |node: &N| -> EagerHeap<H, R> {
                        EagerHeap {
                            heap: fold_for_init.init(node),
                            children: Vec::new(),
                            max_child_height: 0,
                        }
                    },

                    |heap: &mut EagerHeap<H, R>, child: &EagerResult<R>| {
                        heap.children.push(child.completion.clone());
                        if child.height > heap.max_child_height {
                            heap.max_child_height = child.height;
                        }
                    },

                    move |heap: &EagerHeap<H, R>| -> EagerResult<R> {
                        let vh = view_slot.get();
                        let completion = Completion::new();
                        let n_children = heap.children.len();
                        let my_height = if n_children == 0 { 0 } else { heap.max_child_height + 1 };
                        let go_sequential = n_children < min_fork || my_height < min_height;

                        if n_children == 0 {
                            let h = heap.heap.clone();
                            let fp = fold_ptr;
                            let comp = completion.clone();
                            vh.submit(Box::new(move || {
                                comp.set(fp.finalize(&h));
                            }));
                        } else if go_sequential {
                            let mut h = heap.heap.clone();
                            let fp = fold_ptr;
                            for child in &heap.children {
                                let r = child.wait(vh.clone());
                                fp.accumulate(&mut h, &r);
                            }
                            completion.set(fp.finalize(&h));
                        } else {
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

            |n: &N| n.clone(),

            // unwrap: wait for root, clear slot, drop view + fold
            move |result: EagerResult<R>| {
                let vh = view_slot_clear.get().clone();
                let r = result.completion.wait(vh);
                unsafe { *view_slot_clear.inner_raw() = None; }
                let _fold = stash_read.borrow_mut().take();
                let _view = view_stash_read.borrow_mut().take();
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
