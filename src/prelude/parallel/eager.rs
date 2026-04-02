//! Pipelined eager parallel fold as a Lift.
//!
//! Continuation-passing: no task ever waits/blocks.
//!
//! Phase 1 (fused): depth-first traversal runs fold.init per node.
//! Leaf finalize → submit fin(heap) to pool, result stored in Completion.
//! Interior finalize → create Collector, attach to each child's Completion.
//!
//! When a child's pool task completes, it delivers its result via the
//! type-erased parent callback. If I'm the last child, run parent's
//! acc+fin INLINE on my thread — no new task submission.
//!
//! The chain propagates upward: leaf completes → notifies parent →
//! parent completes → notifies grandparent → ... → root done.
//! No blocking anywhere except wait() (caller helps pool while waiting).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::cata::Lift;
use crate::domain::Shared;
use crate::fold;
use super::pool::{WorkPool, WorkPoolSpec};

// ── Completion: result slot + type-erased parent callback ──

struct CompletionInner<R> {
    result: Mutex<Option<R>>,
    /// Type-erased parent notification callback. Set by the parent's
    /// finalize via attach_parent(). The callback captures the parent
    /// Collector and child index, erasing H from Completion's type.
    parent: Mutex<Option<Box<dyn FnOnce(R) + Send>>>,
}

struct Completion<R> {
    inner: Arc<CompletionInner<R>>,
}

impl<R> Clone for Completion<R> {
    fn clone(&self) -> Self { Completion { inner: self.inner.clone() } }
}

impl<R: Clone + Send + Sync + 'static> Completion<R> {
    fn new() -> Self {
        Completion { inner: Arc::new(CompletionInner {
            result: Mutex::new(None),
            parent: Mutex::new(None),
        })}
    }

    /// Called by the pool worker when computation finishes.
    /// If a parent callback is attached, invokes it with the result.
    fn set(&self, value: R) {
        let callback = {
            let mut result = self.inner.result.lock().unwrap();
            *result = Some(value.clone());
            self.inner.parent.lock().unwrap().take()
        };
        if let Some(cb) = callback {
            cb(value);
        }
    }

    /// Attach a parent notification callback. If the child already
    /// completed, the callback fires immediately (inline).
    ///
    /// Lock ordering: result first, then parent — same as set().
    fn attach_parent(&self, callback: Box<dyn FnOnce(R) + Send>) {
        let result_guard = self.inner.result.lock().unwrap();
        if let Some(ref r) = *result_guard {
            let r = r.clone();
            drop(result_guard);
            callback(r);
        } else {
            let mut parent_guard = self.inner.parent.lock().unwrap();
            *parent_guard = Some(callback);
            drop(parent_guard);
            drop(result_guard);
        }
    }

    fn get(&self) -> Option<R> {
        self.inner.result.lock().unwrap().clone()
    }

    /// Block until ready, helping the pool while waiting.
    fn wait(&self, pool: &WorkPool) -> R {
        loop {
            if let Some(r) = self.get() { return r; }
            if !pool.try_run_one() { std::hint::spin_loop(); }
        }
    }
}

// ── Collector: reactive parent computation ────────────────

/// Counts down as children complete. The LAST child runs the parent's
/// accumulate + finalize INLINE on its thread — no task, no blocking.
struct Collector<H, R> {
    remaining: AtomicUsize,
    heap: Mutex<H>,
    child_results: Mutex<Vec<Option<R>>>,
    parent_completion: Completion<R>,
    acc: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    fin: Arc<dyn Fn(&H) -> R + Send + Sync>,
}

impl<H: Send + 'static, R: Clone + Send + Sync + 'static> Collector<H, R> {
    fn child_done(self: &Arc<Self>, child_index: usize, result: R) {
        {
            let mut results = self.child_results.lock().unwrap();
            results[child_index] = Some(result);
        }
        let prev = self.remaining.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            let mut h = self.heap.lock().unwrap();
            let results = self.child_results.lock().unwrap();
            for r in results.iter() {
                (self.acc)(&mut h, r.as_ref().unwrap());
            }
            let result = (self.fin)(&h);
            drop(results);
            drop(h);
            self.parent_completion.set(result);
        }
    }
}

// ── Lifted types ──────────────────────────────────────────

pub struct EagerHeap<H, R> {
    heap: H,
    children: Vec<Completion<R>>,
}

pub struct EagerResult<R> {
    completion: Completion<R>,
}

impl<R> Clone for EagerResult<R> {
    fn clone(&self) -> Self { EagerResult { completion: self.completion.clone() } }
}

// ── ParEager ──────────────────────────────────────────────

pub struct ParEager;

impl ParEager {
    pub fn lift<N, H, R>(pool: &Arc<WorkPool>) -> Lift<Shared, N, H, R, N, EagerHeap<H, R>, EagerResult<R>>
    where
        N: Clone + 'static,
        H: Clone + Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        let pool_for_lift = pool.clone();
        let pool_for_unwrap = pool.clone();

        Lift::new(
            |treeish| treeish,

            move |original_fold: fold::Fold<N, H, R>| {
                let f_init = original_fold.clone();
                let f_acc = original_fold.impl_accumulate.clone();
                let f_fin = original_fold.impl_finalize.clone();
                let pool = pool_for_lift.clone();

                fold::fold(
                    move |node: &N| -> EagerHeap<H, R> {
                        EagerHeap { heap: f_init.init(node), children: Vec::new() }
                    },

                    |heap: &mut EagerHeap<H, R>, child: &EagerResult<R>| {
                        heap.children.push(child.completion.clone());
                    },

                    move |heap: &EagerHeap<H, R>| -> EagerResult<R> {
                        let completion = Completion::new();
                        let n_children = heap.children.len();

                        if n_children == 0 {
                            // LEAF: submit finalize to pool
                            let h = heap.heap.clone();
                            let fin = f_fin.clone();
                            let comp = completion.clone();
                            pool.submit(Box::new(move || {
                                comp.set(fin(&h));
                            }));
                        } else {
                            // INTERIOR: create collector, attach to children
                            let collector = Arc::new(Collector {
                                remaining: AtomicUsize::new(n_children),
                                heap: Mutex::new(heap.heap.clone()),
                                child_results: Mutex::new(
                                    (0..n_children).map(|_| None).collect()
                                ),
                                parent_completion: completion.clone(),
                                acc: f_acc.clone(),
                                fin: f_fin.clone(),
                            });

                            for (idx, child_comp) in heap.children.iter().enumerate() {
                                let coll = collector.clone();
                                child_comp.attach_parent(Box::new(move |result| {
                                    coll.child_done(idx, result);
                                }));
                            }
                        }

                        EagerResult { completion }
                    },
                )
            },

            |n: &N| n.clone(),

            move |result: EagerResult<R>| {
                result.completion.wait(&pool_for_unwrap)
            },
        )
    }

    pub fn with<N, H, R, Ret>(
        spec: WorkPoolSpec,
        f: impl FnOnce(&Lift<Shared, N, H, R, N, EagerHeap<H, R>, EagerResult<R>>) -> Ret,
    ) -> Ret
    where
        N: Clone + 'static,
        H: Clone + Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        WorkPool::with(spec, |pool| f(&Self::lift(pool)))
    }
}
