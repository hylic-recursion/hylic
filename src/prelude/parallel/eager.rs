//! Pipelined eager parallel fold as a Lift.
//!
//! Continuation-passing: no task ever waits/blocks.
//!
//! Phase 1 (fused): depth-first traversal runs fold.init per node.
//! Leaf finalize → submit fin(heap) to pool, result stored in Completion.
//! Interior finalize → create Collector, attach to each child's Completion.
//!
//! When a child's pool task completes, it checks: does this Completion
//! have a parent Collector attached? If so, deliver the result. If I'm
//! the last child, run parent's acc+fin INLINE — no new task.
//!
//! The chain propagates upward: leaf completes → notifies parent →
//! parent completes → notifies grandparent → ... → root done.
//! No blocking anywhere except unwrap (caller helps pool while waiting).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::cata::Lift;
use crate::fold;
use super::pool::{WorkPool, WorkPoolSpec};

// ── Completion: result slot + optional parent link ────────

/// Holds a result (set by pool) and an optional link to the parent
/// collector that should be notified when the result is ready.
struct CompletionInner<R> {
    result: Mutex<Option<R>>,
    /// Set by the parent's finalize. When the result arrives, if
    /// this is Some, we call collector.child_done().
    parent: Mutex<Option<(Arc<Collector<R>>, usize)>>,
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
    /// If a parent collector is attached, notifies it.
    fn set(&self, value: R) {
        let parent_info = {
            let mut result = self.inner.result.lock().unwrap();
            *result = Some(value.clone());
            self.inner.parent.lock().unwrap().take()
        };
        // If parent is waiting for us, deliver now
        if let Some((collector, idx)) = parent_info {
            collector.child_done(idx, value);
        }
    }

    /// Called by the parent's finalize to attach itself.
    /// If the child already completed, delivers immediately.
    fn attach_parent(&self, collector: Arc<Collector<R>>, child_index: usize) {
        let existing_result = {
            let mut parent = self.inner.parent.lock().unwrap();
            let result = self.inner.result.lock().unwrap();
            if result.is_some() {
                // Child already done — deliver inline
                Some(result.clone().unwrap())
            } else {
                // Child not done yet — store the link for later
                *parent = Some((collector.clone(), child_index));
                None
            }
        };
        if let Some(r) = existing_result {
            collector.child_done(child_index, r);
        }
    }

    /// Get the result if ready (non-blocking).
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
/// accumulate + finalize INLINE on its thread. No task, no blocking.
struct Collector<R> {
    remaining: AtomicUsize,
    state: Mutex<CollectorState<R>>,
    parent_completion: Completion<R>,
    acc: Arc<dyn Fn(&mut Vec<R>, &R) + Send + Sync>,
    fin: Arc<dyn Fn(&Vec<R>) -> R + Send + Sync>,
}

struct CollectorState<R> {
    child_results: Vec<Option<R>>,
}

impl<R: Clone + Send + Sync + 'static> Collector<R> {
    /// Called when a child completes. If last child: run parent computation.
    fn child_done(self: &Arc<Self>, child_index: usize, result: R) {
        {
            let mut state = self.state.lock().unwrap();
            state.child_results[child_index] = Some(result);
        }
        let prev = self.remaining.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            // All children done — run parent's computation inline
            let state = self.state.lock().unwrap();
            let mut collected = Vec::new();
            for r in &state.child_results {
                collected.push(r.as_ref().unwrap().clone());
            }
            drop(state);
            let mut heap = collected.clone();
            for r in &collected {
                (self.acc)(&mut heap, r);
            }
            let result = (self.fin)(&heap);
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
    pub fn lift<N, H, R>(pool: &Arc<WorkPool>) -> Lift<N, H, R, N, EagerHeap<H, R>, EagerResult<R>>
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
                let f_acc_raw = original_fold.impl_accumulate.clone();
                let f_fin_raw = original_fold.impl_finalize.clone();
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
                            let fin = f_fin_raw.clone();
                            let comp = completion.clone();
                            pool.submit(Box::new(move || {
                                comp.set(fin(&h));
                            }));
                        } else {
                            // INTERIOR: create collector, attach to children
                            let acc = f_acc_raw.clone();
                            let fin = f_fin_raw.clone();
                            let h = heap.heap.clone();

                            // The collector's acc/fin work on the original H
                            // We wrap them to work with collected Vec<R>
                            let collector = Arc::new(Collector {
                                remaining: AtomicUsize::new(n_children),
                                state: Mutex::new(CollectorState {
                                    child_results: (0..n_children).map(|_| None).collect(),
                                }),
                                parent_completion: completion.clone(),
                                acc: Arc::new(move |_heap: &mut Vec<R>, _r: &R| {
                                    // accumulation happens below in child_done override
                                }),
                                fin: Arc::new(move |_heap: &Vec<R>| -> R {
                                    unreachable!() // we override child_done below
                                }),
                            });

                            // Override: use a proper collector that holds H
                            let collector = Arc::new(ProperCollector {
                                remaining: AtomicUsize::new(n_children),
                                heap: Mutex::new(h),
                                child_results: Mutex::new(
                                    (0..n_children).map(|_| None).collect()
                                ),
                                parent_completion: completion.clone(),
                                acc,
                                fin,
                            });

                            for (idx, child_comp) in heap.children.iter().enumerate() {
                                child_comp.attach_proper_parent(collector.clone(), idx);
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
        f: impl FnOnce(&Lift<N, H, R, N, EagerHeap<H, R>, EagerResult<R>>) -> Ret,
    ) -> Ret
    where
        N: Clone + 'static,
        H: Clone + Send + Sync + 'static,
        R: Clone + Send + Sync + 'static,
    {
        WorkPool::with(spec, |pool| f(&Self::lift(pool)))
    }
}

// ── ProperCollector: holds H, does real acc+fin ───────────

struct ProperCollector<H, R> {
    remaining: AtomicUsize,
    heap: Mutex<H>,
    child_results: Mutex<Vec<Option<R>>>,
    parent_completion: Completion<R>,
    acc: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    fin: Arc<dyn Fn(&H) -> R + Send + Sync>,
}

impl<H: Send + 'static, R: Clone + Send + Sync + 'static> ProperCollector<H, R> {
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
            self.parent_completion.set(result);
        }
    }
}

impl<R: Clone + Send + Sync + 'static> CompletionInner<R> {
    fn attach_proper_parent_inner<H: Send + 'static>(&self, collector: Arc<ProperCollector<H, R>>, child_index: usize) {
        let existing_result = {
            let result = self.result.lock().unwrap();
            result.clone()
        };
        if let Some(r) = existing_result {
            collector.child_done(child_index, r);
        } else {
            // Store as a type-erased callback
            let mut parent = self.parent.lock().unwrap();
            // We need to store a type-erased notifier
            // This is tricky because Collector is generic over H...
            // Let's use a different approach
            todo!("need type-erased parent notification")
        }
    }
}
