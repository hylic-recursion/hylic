//! CPS walk: void traversal with defunctionalized continuations.
//!
//! Reynolds/Danvy defunctionalization applied to the hylomorphism:
//! walk doesn't return R — it delivers to a Cont (the defunctionalized
//! continuation). fire_cont is the apply interpreter (trampolined).
//! The SlotChain's reactive try_advance fires accumulate as results
//! arrive mid-traversal.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::ops::{FoldOps, TreeOps};
use crate::prelude::parallel::pool::{PoolExecView, ViewHandle};
use super::slot_chain::{SlotChain, SlotRef};

// ── Lifetime-erased context ───────────────────────────

/// Carries fold + graph + pool handle through submitted closures.
/// Raw pointers erase lifetimes; the scoped pool guarantees validity.
struct WalkCtx<F, G> {
    fold: *const F,
    graph: *const G,
    handle: ViewHandle,
}

impl<F, G> Clone for WalkCtx<F, G> {
    fn clone(&self) -> Self {
        WalkCtx { fold: self.fold, graph: self.graph, handle: self.handle.clone() }
    }
}

// SAFETY: Raw pointers are valid for the scoped pool's lifetime.
// fold/graph are shared immutably across threads (same as SyncRef).
// ViewHandle is Arc-based (Send+Sync).
unsafe impl<F, G> Send for WalkCtx<F, G> {}
unsafe impl<F, G> Sync for WalkCtx<F, G> {}

impl<F, G> WalkCtx<F, G> {
    /// # Safety
    /// Caller must ensure the fold pointer is still valid.
    unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }

    /// # Safety
    /// Caller must ensure the graph pointer is still valid.
    unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
}

// ── Defunctionalized continuation ─────────────────────

/// Sum type: one variant per continuation call site.
/// Slot = "deliver to parent chain slot" (interior frame).
/// Root = "store final result" (terminal).
enum Cont<H, R> {
    Slot { node: Arc<ChainNode<H, R>>, slot: SlotRef },
    Root(Arc<RootCell<R>>),
}

// SAFETY: Arc<ChainNode> is Send when ChainNode is Send+Sync.
// Arc<RootCell> likewise. SlotRef is Copy.
unsafe impl<H, R: Send> Send for Cont<H, R> {}

/// Chain frame: SlotChain + parent continuation.
/// The parent cont is taken exactly once by the done-CAS winner.
struct ChainNode<H, R> {
    chain: SlotChain<H, R>,
    parent_cont: UnsafeCell<Option<Cont<H, R>>>,
}

// SAFETY: parent_cont written once (construction), taken once (by done-CAS winner).
// SlotChain is Send+Sync (heap access serialized by cursor CAS).
unsafe impl<H, R: Send> Send for ChainNode<H, R> {}
unsafe impl<H, R: Send> Sync for ChainNode<H, R> {}

impl<H, R> ChainNode<H, R> {
    fn new(heap: H, cont: Cont<H, R>) -> Self {
        ChainNode {
            chain: SlotChain::new(heap),
            parent_cont: UnsafeCell::new(Some(cont)),
        }
    }

    /// Take the parent continuation. Called exactly once by the finalization winner.
    fn take_parent_cont(&self) -> Cont<H, R> {
        unsafe { (*self.parent_cont.get()).take().expect("parent cont already taken") }
    }
}

// ── Root result cell ──────────────────────────────────

/// One-shot: set by finalization cascade, read by run_fold.
struct RootCell<R> {
    result: UnsafeCell<Option<R>>,
    done: AtomicBool,
}

// SAFETY: Single-producer (done-CAS winner), single-consumer (run_fold).
unsafe impl<R: Send> Send for RootCell<R> {}
unsafe impl<R: Send> Sync for RootCell<R> {}

impl<R> RootCell<R> {
    fn new() -> Self {
        RootCell { result: UnsafeCell::new(None), done: AtomicBool::new(false) }
    }

    fn set(&self, r: R) {
        unsafe { *self.result.get() = Some(r); }
        self.done.store(true, Ordering::Release);
    }

    fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    fn take(&self) -> R {
        unsafe { (*self.result.get()).take().expect("root result not set") }
    }
}

// ── apply(cont, result) — trampolined ─────────────────

/// Fire a continuation: deliver result to the target.
/// Trampolined: cascades up through parent continuations in a loop.
fn fire_cont<N, H, R, F: FoldOps<N, H, R>, G>(
    ctx: &WalkCtx<F, G>,
    mut cont: Cont<H, R>,
    mut result: R,
) {
    loop {
        match cont {
            Cont::Root(cell) => {
                cell.set(result);
                return;
            }
            Cont::Slot { node, slot } => {
                let fold = unsafe { ctx.fold_ref() };
                match node.chain.deliver_cps(slot, result, fold) {
                    None => return,
                    Some(finalized) => {
                        cont = node.take_parent_cont();
                        result = finalized;
                    }
                }
            }
        }
    }
}

// ── CPS walk ──────────────────────────────────────────

/// Void walk. Processes node, delivers result to cont.
/// Leaf: finalize → fire_cont immediately.
/// Interior: visit callback submits every child to pool, then set_total.
fn walk_cps<N, H, R, F, G>(ctx: &WalkCtx<F, G>, node: N, cont: Cont<H, R>)
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let fold = unsafe { ctx.fold_ref() };
    let graph = unsafe { ctx.graph_ref() };
    let heap = fold.init(&node);

    let mut chain: Option<Arc<ChainNode<H, R>>> = None;
    let mut heap_opt = Some(heap);
    let mut cont_opt = Some(cont);

    graph.visit(&node, &mut |child: &N| {
        // Lazy chain creation on first child — leaves never allocate.
        if chain.is_none() {
            chain = Some(Arc::new(ChainNode::new(
                heap_opt.take().unwrap(),
                cont_opt.take().unwrap(),
            )));
        }
        let cn = chain.as_ref().unwrap();
        let slot = cn.chain.append_slot();

        let ctx2 = ctx.clone();
        let cn2 = cn.clone();
        let child = child.clone();
        ctx.handle.submit(Box::new(move || {
            walk_cps::<N, H, R, F, G>(&ctx2, child, Cont::Slot { node: cn2, slot });
        }));
    });

    match chain {
        None => {
            // Leaf: no children discovered. Finalize directly.
            let heap = heap_opt.take().unwrap();
            let cont = cont_opt.take().unwrap();
            let result = fold.finalize(&heap);
            fire_cont::<N, H, R, F, G>(ctx, cont, result);
        }
        Some(cn) => {
            // Interior: all children submitted, total known.
            let fold = unsafe { ctx.fold_ref() };
            if let Some(finalized) = cn.chain.set_total_cps(fold) {
                let parent = cn.take_parent_cont();
                fire_cont::<N, H, R, F, G>(ctx, parent, finalized);
            }
        }
    }
}

// ── Entry point ───────────────────────────────────────

pub fn run_fold<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + 'static),
    graph: &(impl TreeOps<N> + 'static),
    root: &N,
    view: &PoolExecView,
) -> R
where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let root_cell = Arc::new(RootCell::new());
    let ctx = WalkCtx {
        fold: fold as *const _,
        graph: graph as *const _,
        handle: view.handle(),
    };

    walk_cps(&ctx, root.clone(), Cont::Root(root_cell.clone()));

    // Help-wait: steal and execute tasks until root result is ready.
    while !root_cell.is_done() {
        if !view.help_once() {
            std::hint::spin_loop();
        }
    }
    root_cell.take()
}
