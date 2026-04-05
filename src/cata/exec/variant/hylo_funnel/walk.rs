//! CPS walk for hylo-funnel: streaming submission, first-child inlining,
//! Cont::Direct cascade fast path for single-child nodes.
//!
//! Streaming: sibling tasks are submitted DURING the visit callback, not after.
//! Workers start DFS into sibling subtrees immediately.
//!
//! First-child inlining: child 0 is saved for inline walk (no Box, no queue).
//! Strategy selection (Direct vs Slot) happens after the visit loop.
//!
//! Cont::Direct: single-child inlined node. The parent's heap travels with
//! the continuation. No ChainNode, no FoldChain, no CAS, no slots.
//! fire_cont accumulates directly and finalizes — sequential speed.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::pool::FunnelPool;
use super::fold_chain::{FoldChain, SlotRef};

// ── Lifetime-erased context ───────────────────────────

struct WalkCtx<F, G> {
    fold: *const F,
    graph: *const G,
    pool: *const FunnelPool,
}

impl<F, G> Clone for WalkCtx<F, G> {
    fn clone(&self) -> Self {
        WalkCtx { fold: self.fold, graph: self.graph, pool: self.pool }
    }
}

unsafe impl<F, G> Send for WalkCtx<F, G> {}
unsafe impl<F, G> Sync for WalkCtx<F, G> {}

impl<F, G> WalkCtx<F, G> {
    unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }
    unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
    unsafe fn pool_ref(&self) -> &FunnelPool { unsafe { &*self.pool } }
}

// ── Defunctionalized continuation ─────────────────────

enum Cont<H, R> {
    /// Terminal: fold result delivered here.
    Root(Arc<RootCell<R>>),
    /// Fast path: single-child inlined node. Heap travels with the
    /// continuation. No FoldChain, no CAS, no slots.
    Direct { heap: H, parent_cont: Box<Cont<H, R>> },
    /// General path: multi-child. Result delivered to a FoldChain slot.
    Slot { node: Arc<ChainNode<H, R>>, slot: SlotRef },
}

unsafe impl<H, R: Send> Send for Cont<H, R> {}

// ── ChainNode (multi-child only) ─────────────────────

struct ChainNode<H, R> {
    chain: FoldChain<H, R>,
    parent_cont: UnsafeCell<Option<Cont<H, R>>>,
}

unsafe impl<H, R: Send> Send for ChainNode<H, R> {}
unsafe impl<H, R: Send> Sync for ChainNode<H, R> {}

impl<H, R> ChainNode<H, R> {
    fn new(heap: H, cont: Cont<H, R>) -> Self {
        ChainNode { chain: FoldChain::new(heap), parent_cont: UnsafeCell::new(Some(cont)) }
    }
    fn take_parent_cont(&self) -> Cont<H, R> {
        unsafe { (*self.parent_cont.get()).take().expect("parent cont already taken") }
    }
}

// ── Root result cell ──────────────────────────────────

struct RootCell<R> {
    result: UnsafeCell<Option<R>>,
    done: AtomicBool,
}

unsafe impl<R: Send> Send for RootCell<R> {}
unsafe impl<R: Send> Sync for RootCell<R> {}

impl<R> RootCell<R> {
    fn new() -> Self { RootCell { result: UnsafeCell::new(None), done: AtomicBool::new(false) } }
    fn set(&self, r: R) {
        unsafe { *self.result.get() = Some(r); }
        self.done.store(true, Ordering::Release);
    }
    fn is_done(&self) -> bool { self.done.load(Ordering::Acquire) }
    fn take(&self) -> R { unsafe { (*self.result.get()).take().expect("root result not set") } }
}

// ── submit_raker ──────────────────────────────────────

fn submit_raker<N, H, R, F, G>(ctx: &WalkCtx<F, G>, node: Arc<ChainNode<H, R>>)
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let ctx2 = ctx.clone();
    let pool = unsafe { ctx.pool_ref() };
    pool.submit(move || {
        let fold = unsafe { ctx2.fold_ref() };
        if let Some(result) = node.chain.rake(fold) {
            let parent = node.take_parent_cont();
            fire_cont::<N, H, R, F, G>(&ctx2, parent, result);
        }
    });
}

// ── fire_cont (trampolined, with Direct fast path) ────

fn fire_cont<N, H, R, F, G>(
    ctx: &WalkCtx<F, G>,
    mut cont: Cont<H, R>,
    mut result: R,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    loop {
        match cont {
            Cont::Root(cell) => {
                cell.set(result);
                return;
            }
            Cont::Direct { mut heap, parent_cont } => {
                // FAST PATH: single-child inlined node.
                // No FoldChain, no CAS, no slots. Sequential speed.
                let fold = unsafe { ctx.fold_ref() };
                fold.accumulate(&mut heap, &result);
                result = fold.finalize(&heap);
                cont = *parent_cont;
            }
            Cont::Slot { node, slot } => {
                // GENERAL PATH: multi-child or submitted child.
                node.chain.deliver(slot, result);
                let fold = unsafe { ctx.fold_ref() };
                match node.chain.rake(fold) {
                    Some(finalized) => {
                        cont = node.take_parent_cont();
                        result = finalized;
                    }
                    None => {
                        submit_raker::<N, H, R, F, G>(ctx, node);
                        return;
                    }
                }
            }
        }
    }
}

// ── CPS walk: streaming submission + Direct fast path ─

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
    let pool = unsafe { ctx.pool_ref() };
    let heap = fold.init(&node);

    // State for streaming child dispatch:
    // - child_count: total children seen so far
    // - first_child: saved for inline walk (never submitted)
    // - chain: created lazily on second child (multi-child case)
    // - heap/cont held in Options, taken when ChainNode is created
    let mut child_count = 0u32;
    let mut first_child: Option<N> = None;
    let mut chain: Option<Arc<ChainNode<H, R>>> = None;
    let mut heap_opt = Some(heap);
    let mut cont_opt = Some(cont);

    graph.visit(&node, &mut |child: &N| {
        child_count += 1;

        if child_count == 1 {
            // First child: save for inline walk. Don't allocate anything.
            first_child = Some(child.clone());
        } else {
            // Second+ child: streaming submission.
            if child_count == 2 {
                // Transition to multi-child: create FoldChain now.
                chain = Some(Arc::new(ChainNode::new(
                    heap_opt.take().unwrap(),
                    cont_opt.take().unwrap(),
                )));
                // Register slot 0 for the saved first child.
                chain.as_ref().unwrap().chain.append_slot();
            }
            let cn = chain.as_ref().unwrap();
            let slot = cn.chain.append_slot();
            // SUBMIT IMMEDIATELY — worker starts DFS into this subtree now.
            let ctx2 = ctx.clone();
            let cn2 = cn.clone();
            let child = child.clone();
            pool.submit(move || {
                walk_cps::<N, H, R, F, G>(&ctx2, child, Cont::Slot { node: cn2, slot });
            });
        }
    });

    // Post-visit: choose strategy based on final child count.
    match child_count {
        0 => {
            // Leaf: no children. Finalize and deliver.
            let heap = heap_opt.take().unwrap();
            let cont = cont_opt.take().unwrap();
            let result = fold.finalize(&heap);
            fire_cont::<N, H, R, F, G>(ctx, cont, result);
        }
        1 => {
            // Single child: Cont::Direct — zero overhead cascade.
            // No FoldChain, no Arc, no atomics, no slots.
            let child = first_child.unwrap();
            let heap = heap_opt.take().unwrap();
            let parent_cont = cont_opt.take().unwrap();
            walk_cps::<N, H, R, F, G>(ctx, child, Cont::Direct {
                heap,
                parent_cont: Box::new(parent_cont),
            });
        }
        _ => {
            // Multi-child: FoldChain with CAS sweep.
            let cn = chain.unwrap();
            cn.chain.set_total();
            let child = first_child.unwrap();
            // Walk child 0 inline with Cont::Slot for slot 0.
            walk_cps::<N, H, R, F, G>(ctx, child, Cont::Slot {
                node: cn,
                slot: SlotRef(0),
            });
        }
    }
}

// ── Entry point ───────────────────────────────────────

pub fn run_fold<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + 'static),
    graph: &(impl TreeOps<N> + 'static),
    root: &N,
    pool: &FunnelPool,
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
        pool: pool as *const _,
    };

    walk_cps(&ctx, root.clone(), Cont::Root(root_cell.clone()));

    let mut spins = 0u64;
    while !root_cell.is_done() {
        if pool.help_once() {
            spins = 0;
        } else {
            spins += 1;
            if spins > 10_000_000 {
                panic!("run_fold hung: root_done={}", root_cell.is_done());
            }
            std::hint::spin_loop();
        }
    }
    root_cell.take()
}
