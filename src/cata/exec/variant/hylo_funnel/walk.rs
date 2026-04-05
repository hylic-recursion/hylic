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
//!
//! Arena-allocated ChainNodes: multi-child nodes live in a pre-allocated
//! slab. ArenaIdx is Copy (u32) — no Arc, no refcounting, bulk-freed.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::pool::FunnelPool;
use super::fold_chain::{FoldChain, SlotRef};
use super::arena::{Arena, ArenaIdx};

// ── Lifetime-erased context ───────────────────────────

struct WalkCtx<F, G, H, R> {
    fold: *const F,
    graph: *const G,
    pool: *const FunnelPool,
    arena: *const Arena<ChainNode<H, R>>,
}

impl<F, G, H, R> Clone for WalkCtx<F, G, H, R> {
    fn clone(&self) -> Self {
        WalkCtx { fold: self.fold, graph: self.graph, pool: self.pool, arena: self.arena }
    }
}

unsafe impl<F, G, H, R> Send for WalkCtx<F, G, H, R> {}
unsafe impl<F, G, H, R> Sync for WalkCtx<F, G, H, R> {}

impl<F, G, H, R> WalkCtx<F, G, H, R> {
    unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }
    unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
    unsafe fn pool_ref(&self) -> &FunnelPool { unsafe { &*self.pool } }
    unsafe fn arena_ref(&self) -> &Arena<ChainNode<H, R>> { unsafe { &*self.arena } }
}

// ── Defunctionalized continuation ─────────────────────

enum Cont<H, R> {
    /// Terminal: fold result delivered here.
    Root(Arc<RootCell<R>>),
    /// Fast path: single-child inlined node. Heap travels with the
    /// continuation. No FoldChain, no CAS, no slots.
    Direct { heap: H, parent_cont: Box<Cont<H, R>> },
    /// General path: multi-child. Result delivered to a FoldChain slot.
    /// ArenaIdx is Copy — no refcounting.
    Slot { node: ArenaIdx, slot: SlotRef },
}

unsafe impl<H, R: Send> Send for Cont<H, R> {}

// ── ChainNode (multi-child only, arena-allocated) ────

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

fn submit_raker<N, H, R, F, G>(ctx: &WalkCtx<F, G, H, R>, node_idx: ArenaIdx)
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
        let arena = unsafe { ctx2.arena_ref() };
        let node = unsafe { arena.get(node_idx) };
        if let Some(result) = node.chain.rake(fold) {
            let parent = node.take_parent_cont();
            fire_cont::<N, H, R, F, G>(&ctx2, parent, result);
        }
    });
}

// ── fire_cont (trampolined, with Direct fast path) ────

fn fire_cont<N, H, R, F, G>(
    ctx: &WalkCtx<F, G, H, R>,
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
            Cont::Slot { node: node_idx, slot } => {
                // GENERAL PATH: multi-child or submitted child.
                let arena = unsafe { ctx.arena_ref() };
                let node = unsafe { arena.get(node_idx) };
                node.chain.deliver(slot, result);
                let fold = unsafe { ctx.fold_ref() };
                match node.chain.rake(fold) {
                    Some(finalized) => {
                        cont = node.take_parent_cont();
                        result = finalized;
                    }
                    None => {
                        submit_raker::<N, H, R, F, G>(ctx, node_idx);
                        return;
                    }
                }
            }
        }
    }
}

// ── CPS walk: streaming submission + Direct fast path ─

fn walk_cps<N, H, R, F, G>(ctx: &WalkCtx<F, G, H, R>, node: N, cont: Cont<H, R>)
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
    let arena = unsafe { ctx.arena_ref() };
    let heap = fold.init(&node);

    let mut child_count = 0u32;
    let mut first_child: Option<N> = None;
    let mut chain_idx: Option<ArenaIdx> = None;
    let mut heap_opt = Some(heap);
    let mut cont_opt = Some(cont);

    graph.visit(&node, &mut |child: &N| {
        child_count += 1;

        if child_count == 1 {
            first_child = Some(child.clone());
        } else {
            if child_count == 2 {
                // Transition to multi-child: arena-allocate ChainNode.
                let cn = ChainNode::new(
                    heap_opt.take().unwrap(),
                    cont_opt.take().unwrap(),
                );
                let idx = arena.alloc(cn);
                let node_ref = unsafe { arena.get(idx) };
                node_ref.chain.append_slot(); // slot 0 for first child
                chain_idx = Some(idx);
            }
            let idx = chain_idx.unwrap();
            let node_ref = unsafe { arena.get(idx) };
            let slot = node_ref.chain.append_slot();
            let ctx2 = ctx.clone();
            let child = child.clone();
            // ArenaIdx is Copy — no Arc clone.
            pool.submit(move || {
                walk_cps::<N, H, R, F, G>(&ctx2, child, Cont::Slot { node: idx, slot });
            });
        }
    });

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
            let idx = chain_idx.unwrap();
            let cn = unsafe { arena.get(idx) };
            cn.chain.set_total();
            let child = first_child.unwrap();
            // Walk child 0 inline with Cont::Slot for slot 0.
            walk_cps::<N, H, R, F, G>(ctx, child, Cont::Slot {
                node: idx,
                slot: SlotRef(0),
            });
        }
    }
}

// ── Entry point ───────────────────────────────────────

/// Default arena capacity. Covers trees up to ~2000 interior nodes.
const ARENA_CAPACITY: usize = 4096;

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
    let arena = Arena::<ChainNode<H, R>>::new(ARENA_CAPACITY);
    let root_cell = Arc::new(RootCell::new());
    let ctx = WalkCtx {
        fold: fold as *const _,
        graph: graph as *const _,
        pool: pool as *const _,
        arena: &arena as *const _,
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
