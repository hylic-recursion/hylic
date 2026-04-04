//! CPS walk with inline-first raker.
//!
//! walk_cps is void — delivers results through defunctionalized continuations.
//! fire_cont tries to rake inline (same thread, zero alloc). Falls back to
//! submitting a raker task on CAS failure.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::ops::{FoldOps, TreeOps};
use crate::prelude::parallel::pool::{PoolExecView, ViewHandle};
use super::fold_chain::{FoldChain, SlotRef};

// ── Lifetime-erased context ───────────────────────────

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

unsafe impl<F, G> Send for WalkCtx<F, G> {}
unsafe impl<F, G> Sync for WalkCtx<F, G> {}

impl<F, G> WalkCtx<F, G> {
    unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }
    unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
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

/// Fallback: submit a raker task when inline CAS failed.
fn submit_raker<N, H, R, F, G>(ctx: &WalkCtx<F, G>, node: Arc<ChainNode<H, R>>)
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let ctx2 = ctx.clone();
    ctx.handle.submit(move || {
        let fold = unsafe { ctx2.fold_ref() };
        if let Some(result) = node.chain.rake(fold) {
            let parent = node.take_parent_cont();
            fire_cont::<N, H, R, F, G>(&ctx2, parent, result);
        }
    });
}

// ── fire_cont (trampolined, inline-first) ─────────────

/// Deliver result to continuation target. Tries inline rake first.
/// On CAS failure, submits a raker task as fallback.
/// Trampolined: cascades without stack growth.
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
                        // Trampoline: loop to cascade inline
                    }
                    None => {
                        // CAS failed or chain not complete.
                        // Submit raker task as fallback.
                        submit_raker::<N, H, R, F, G>(ctx, node);
                        return;
                    }
                }
            }
        }
    }
}

// ── CPS walk ──────────────────────────────────────────

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
        ctx.handle.submit(move || {
            walk_cps::<N, H, R, F, G>(&ctx2, child, Cont::Slot { node: cn2, slot });
        });
    });

    match chain {
        None => {
            // Leaf
            let heap = heap_opt.take().unwrap();
            let cont = cont_opt.take().unwrap();
            let result = fold.finalize(&heap);
            fire_cont::<N, H, R, F, G>(ctx, cont, result);
        }
        Some(cn) => {
            // Interior: children stream finished.
            cn.chain.set_total();
            // Inline rake for the set_total event.
            let fold = unsafe { ctx.fold_ref() };
            match cn.chain.rake(fold) {
                Some(finalized) => {
                    let parent = cn.take_parent_cont();
                    fire_cont::<N, H, R, F, G>(ctx, parent, finalized);
                }
                None => {
                    // CAS failed or chain not complete. Submit fallback.
                    submit_raker::<N, H, R, F, G>(ctx, cn);
                }
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

    let mut spins = 0u64;
    while !root_cell.is_done() {
        if view.help_once() {
            spins = 0;
        } else {
            spins += 1;
            if spins > 10_000_000 {
                panic!("run_fold hung: deque_len={}, views={}, root_done={}",
                    view.deque_len(), view.views_count(), root_cell.is_done());
            }
            std::hint::spin_loop();
        }
    }
    root_cell.take()
}
