//! Eager parallel fold as a Lift, backed by a WorkPool.
//!
//! Phase 1 (via provided Exec): extracts heaps into an EagerNode tree.
//! Phase 2 (in unwrap): recursive fork-join on the heap tree.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::cata::Lift;
use crate::fold;
use super::pool::{WorkPool, WorkPoolSpec};

// ── Phase 1 types ──────────────────────────────────────────

/// Node in the extracted heap tree.
pub struct EagerNode<H> {
    heap: H,
    children: Vec<Arc<EagerNode<H>>>,
}

/// Handle carrying the heap tree + fold operations for Phase 2.
pub struct EagerHandle<H, R> {
    root: Arc<EagerNode<H>>,
    fold_acc: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    fold_fin: Arc<dyn Fn(&H) -> R + Send + Sync>,
}

impl<H, R> Clone for EagerHandle<H, R> {
    fn clone(&self) -> Self {
        EagerHandle {
            root: self.root.clone(),
            fold_acc: self.fold_acc.clone(),
            fold_fin: self.fold_fin.clone(),
        }
    }
}

// ── Fork-join result collection ────────────────────────────

struct ForkResults<R> {
    slots: Vec<UnsafeCell<Option<R>>>,
    remaining: AtomicUsize,
}

// SAFETY: each slot is written by one thread, read by one thread
// (the parent), with atomic counter providing happens-before ordering.
unsafe impl<R: Send> Sync for ForkResults<R> {}

impl<R> ForkResults<R> {
    fn new(n: usize) -> Self {
        ForkResults {
            slots: (0..n).map(|_| UnsafeCell::new(None)).collect(),
            remaining: AtomicUsize::new(n),
        }
    }

    /// SAFETY: caller must ensure exclusive write access to slot `i`.
    unsafe fn write(&self, i: usize, value: R) {
        unsafe { *self.slots[i].get() = Some(value); }
        self.remaining.fetch_sub(1, Ordering::Release);
    }

    fn is_done(&self) -> bool {
        self.remaining.load(Ordering::Acquire) == 0
    }

    /// SAFETY: slot `i` must have been written, and is_done() must be true.
    unsafe fn get(&self, i: usize) -> &R {
        unsafe { (*self.slots[i].get()).as_ref().unwrap() }
    }
}

// ── ParEager: the strategy ─────────────────────────────────

/// Eager parallel strategy. Extracts heaps into a tree during Phase 1,
/// then executes bottom-up with fork-join parallelism in Phase 2.
pub struct ParEager;

impl ParEager {
    /// Create a Lift backed by an existing pool.
    pub fn lift<N, H, R>(pool: &Arc<WorkPool>) -> Lift<N, H, R, N, EagerNode<H>, EagerHandle<H, R>>
    where
        N: Clone + 'static,
        H: Clone + Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        let pool = pool.clone();
        Lift::new(
            |treeish| treeish,

            move |original_fold: fold::Fold<N, H, R>| {
                let f_init = original_fold.clone();
                let f_acc = original_fold.impl_accumulate.clone();
                let f_fin = original_fold.impl_finalize.clone();

                fold::fold(
                    move |node: &N| -> EagerNode<H> {
                        EagerNode { heap: f_init.init(node), children: Vec::new() }
                    },
                    |heap: &mut EagerNode<H>, child: &EagerHandle<H, R>| {
                        heap.children.push(child.root.clone());
                    },
                    move |heap: &EagerNode<H>| -> EagerHandle<H, R> {
                        EagerHandle {
                            root: Arc::new(EagerNode {
                                heap: heap.heap.clone(),
                                children: heap.children.clone(),
                            }),
                            fold_acc: f_acc.clone(),
                            fold_fin: f_fin.clone(),
                        }
                    },
                )
            },

            |n: &N| n.clone(),

            move |handle: EagerHandle<H, R>| {
                exec_node(&handle.root, &pool, &handle.fold_acc, &handle.fold_fin)
            },
        )
    }

    /// Convenience: create a scoped pool, build the lift, pass it to `f`.
    pub fn with<N, H, R, Ret>(
        spec: WorkPoolSpec,
        f: impl FnOnce(&Lift<N, H, R, N, EagerNode<H>, EagerHandle<H, R>>) -> Ret,
    ) -> Ret
    where
        N: Clone + 'static,
        H: Clone + Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        WorkPool::with(spec, |pool| f(&Self::lift(pool)))
    }
}

// ── Phase 2: recursive fork-join execution ─────────────────

fn exec_node<H: Clone + Send + Sync + 'static, R: Send + 'static>(
    node: &EagerNode<H>,
    pool: &Arc<WorkPool>,
    acc: &Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    fin: &Arc<dyn Fn(&H) -> R + Send + Sync>,
) -> R {
    let mut h = node.heap.clone();
    let n = node.children.len();

    if n <= 1 {
        for c in &node.children {
            let r = exec_node(c, pool, acc, fin);
            (acc)(&mut h, &r);
        }
    } else {
        let results = Arc::new(ForkResults::<R>::new(n - 1));

        for i in 0..n - 1 {
            let child = node.children[i].clone();
            let pool_c = pool.clone();
            let acc_c = acc.clone();
            let fin_c = fin.clone();
            let results_c = results.clone();
            pool.submit(Box::new(move || {
                let r = exec_node(&child, &pool_c, &acc_c, &fin_c);
                unsafe { results_c.write(i, r); }
            }));
        }

        let last_result = exec_node(&node.children[n - 1], pool, acc, fin);

        while !results.is_done() {
            if !pool.try_run_one() {
                std::hint::spin_loop();
            }
        }

        for i in 0..n - 1 {
            (acc)(&mut h, unsafe { results.get(i) });
        }
        (acc)(&mut h, &last_result);
    }

    (fin)(&h)
}
