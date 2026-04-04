//! CPS walk for hylo-funnel: hardcoded to FunnelPool, first-child inlining.
//!
//! walk_cps is void — delivers results through defunctionalized continuations.
//! fire_cont rakes inline, trampolines up on success, submits raker on CAS failure.
//!
//! First-child inlining: child 0 is walked inline (no Box, no queue push).
//! Remaining children are submitted as tasks. The thread follows the leftmost
//! spine depth-first with zero queue overhead.

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
    Slot { node: Arc<ChainNode<H, R>>, slot: SlotRef },
    Root(Arc<RootCell<R>>),
}

unsafe impl<H, R: Send> Send for Cont<H, R> {}

// ── ChainNode ────────────────────────────────────────

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

// ── fire_cont (trampolined, inline-first) ─────────────

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
            Cont::Slot { node, slot } => {
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

// ── CPS walk with first-child inlining ───────────────

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

    let mut chain: Option<Arc<ChainNode<H, R>>> = None;
    let mut heap_opt = Some(heap);
    let mut cont_opt = Some(cont);
    let mut first_child: Option<(N, SlotRef)> = None;

    graph.visit(&node, &mut |child: &N| {
        if chain.is_none() {
            chain = Some(Arc::new(ChainNode::new(
                heap_opt.take().unwrap(),
                cont_opt.take().unwrap(),
            )));
        }
        let cn = chain.as_ref().unwrap();
        let slot = cn.chain.append_slot();

        if first_child.is_none() {
            // Keep first child for inline walk — no Box, no queue
            first_child = Some((child.clone(), slot));
        } else {
            // Submit remaining children as tasks
            let ctx2 = ctx.clone();
            let cn2 = cn.clone();
            let child = child.clone();
            pool.submit(move || {
                walk_cps::<N, H, R, F, G>(&ctx2, child, Cont::Slot { node: cn2, slot });
            });
        }
    });

    match chain {
        None => {
            // Leaf: no children
            let heap = heap_opt.take().unwrap();
            let cont = cont_opt.take().unwrap();
            let result = fold.finalize(&heap);
            fire_cont::<N, H, R, F, G>(ctx, cont, result);
        }
        Some(cn) => {
            cn.chain.set_total();

            let (first_node, first_slot) = first_child.unwrap();

            // Try inline rake for the set_total event (covers case where
            // all submitted children already delivered before we get here).
            let fold = unsafe { ctx.fold_ref() };
            match cn.chain.rake(fold) {
                Some(finalized) => {
                    // All children done before we walk child 0.
                    // This is unusual but possible. Cascade result.
                    let parent = cn.take_parent_cont();
                    fire_cont::<N, H, R, F, G>(ctx, parent, finalized);
                    // Still need to walk child 0 — but chain is finalized.
                    // Actually: if chain finalized, child 0's result was already
                    // delivered (by some worker). But we haven't walked child 0
                    // inline yet... wait, child 0 IS the first_child we kept.
                    // If the chain finalized, it means some worker ALSO walked
                    // child 0 — but we didn't submit child 0! Only we have it.
                    //
                    // So this can't happen: child 0 was never submitted, its
                    // slot can't be filled, the chain can't finalize without it.
                    // The rake here can return Some only if total == 0 (no children),
                    // but we're in the Some(cn) branch meaning there IS at least 1 child.
                    //
                    // Therefore: this path is unreachable when first_child is Some.
                    unreachable!("chain finalized before first child walked");
                }
                None => {
                    // Expected: chain not complete (child 0 hasn't delivered yet).
                    // Don't submit a raker here — we're about to walk child 0
                    // inline, which will deliver and trigger the cascade.
                }
            }

            // Walk first child inline — depth-first down the leftmost spine.
            // No Box, no queue push, no context switch.
            walk_cps::<N, H, R, F, G>(ctx, first_node, Cont::Slot { node: cn, slot: first_slot });
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
