//! CPS walk: defunctionalized tasks, packed-ticket streaming sweep.
//!
//! walk_cps is void — delivers results through defunctionalized continuations.
//! fire_cont uses P::Accumulate for compile-time dispatch.
//! fold_done set inside fire_cont(Cont::Root) — CPS completion signal.
//!
//! Generic over P: FunnelPolicy — queue, accumulation, and wake strategies
//! are all resolved at monomorphization time.

use std::sync::atomic::Ordering;
use crate::ops::{FoldOps, TreeOps};
use super::cont::{FunnelTask, Cont, ChainNode};
use super::super::view::FoldView;
use super::super::worker::WorkerCtx;
use super::super::policy::FunnelPolicy;
use super::super::accumulate::AccumulateStrategy;
use super::chain::SlotRef;
use super::super::arena::Arena;
use super::super::cont_arena::ContArena;

// ── Shared immutable context (created once, passed by &ref) ──

pub(crate) struct WalkCtx<F, G, H, R, P: FunnelPolicy> {
    pub(crate) fold: *const F,
    pub(crate) graph: *const G,
    pub(crate) view: *const FoldView,
    pub(crate) chain_arena: *const Arena<ChainNode<H, R>>,
    pub(crate) cont_arena: *const ContArena<Cont<H, R>>,
    pub(crate) _policy: std::marker::PhantomData<P>,
}

unsafe impl<F, G, H, R, P: FunnelPolicy> Send for WalkCtx<F, G, H, R, P> {}
unsafe impl<F, G, H, R, P: FunnelPolicy> Sync for WalkCtx<F, G, H, R, P> {}

impl<F, G, H, R, P: FunnelPolicy> WalkCtx<F, G, H, R, P> {
    pub(crate) unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }
    pub(crate) unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
    pub(crate) unsafe fn view_ref(&self) -> &FoldView { unsafe { &*self.view } }
    pub(crate) unsafe fn chain_arena(&self) -> &Arena<ChainNode<H, R>> { unsafe { &*self.chain_arena } }
    pub(crate) unsafe fn cont_arena(&self) -> &ContArena<Cont<H, R>> { unsafe { &*self.cont_arena } }
}

// ── fire_cont (trampolined) ──────────────────────────

pub(crate) fn fire_cont<N, H, R, F, G, P: FunnelPolicy>(
    ctx: &WalkCtx<F, G, H, R, P>,
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
                let view = unsafe { ctx.view_ref() };
                view.fold_done.store(true, Ordering::Release);
                view.event().notify_all();
                return;
            }
            Cont::Direct { mut heap, parent_idx } => {
                let fold = unsafe { ctx.fold_ref() };
                fold.accumulate(&mut heap, &result);
                result = fold.finalize(&heap);
                cont = unsafe { ctx.cont_arena().take(parent_idx) };
            }
            Cont::Slot { node: node_idx, slot } => {
                let arena = unsafe { ctx.chain_arena() };
                let node = unsafe { arena.get(node_idx) };
                let fold = unsafe { ctx.fold_ref() };
                let delivered = P::Accumulate::deliver(&node.chain, slot, result, fold);
                match delivered {
                    Some(finalized) => {
                        cont = node.take_parent_cont();
                        result = finalized;
                    }
                    None => return,
                }
            }
        }
    }
}

// ── CPS walk ─────────────────────────────────────────

pub(crate) fn walk_cps<N, H, R, F, G, P: FunnelPolicy>(
    wctx: &WorkerCtx<N, H, R, F, G, P>,
    mut node: N,
    mut cont: Cont<H, R>,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let ctx = wctx.ctx;
    loop {
        let fold = unsafe { ctx.fold_ref() };
        let graph = unsafe { ctx.graph_ref() };
        let chain_arena = unsafe { ctx.chain_arena() };
        let cont_arena = unsafe { ctx.cont_arena() };
        let heap = fold.init(&node);

        let mut child_count = 0u32;
        let mut first_child: Option<N> = None;
        let mut chain_idx: Option<super::super::arena::ArenaIdx> = None;
        let mut heap_opt = Some(heap);
        let mut cont_opt = Some(cont);

        wctx.reset_wake();
        graph.visit(&node, &mut |child: &N| {
            child_count += 1;
            if child_count == 1 {
                first_child = Some(child.clone());
            } else {
                if child_count == 2 {
                    let cn = ChainNode::new(heap_opt.take().unwrap(), cont_opt.take().unwrap());
                    let idx = chain_arena.alloc(cn);
                    let node_ref = unsafe { chain_arena.get(idx) };
                    node_ref.chain.append_slot();
                    chain_idx = Some(idx);
                }
                let idx = chain_idx.unwrap();
                let node_ref = unsafe { chain_arena.get(idx) };
                let slot = node_ref.chain.append_slot();
                wctx.push_task(FunnelTask::Walk {
                    child: child.clone(),
                    cont: Cont::Slot { node: idx, slot },
                });
            }
        });

        match child_count {
            0 => {
                let heap = heap_opt.take().unwrap();
                let cont = cont_opt.take().unwrap();
                let result = fold.finalize(&heap);
                fire_cont::<N, H, R, F, G, P>(ctx, cont, result);
                return;
            }
            1 => {
                let child = first_child.unwrap();
                let heap = heap_opt.take().unwrap();
                let parent_cont = cont_opt.take().unwrap();
                let parent_idx = cont_arena.alloc(parent_cont);
                node = child;
                cont = Cont::Direct { heap, parent_idx };
            }
            _ => {
                let idx = chain_idx.unwrap();
                let cn = unsafe { chain_arena.get(idx) };
                let fold = unsafe { ctx.fold_ref() };
                let set_total_result = P::Accumulate::set_total(&cn.chain, fold);
                if let Some(finalized) = set_total_result {
                    let parent = cn.take_parent_cont();
                    fire_cont::<N, H, R, F, G, P>(ctx, parent, finalized);
                    return;
                }
                let child = first_child.unwrap();
                node = child;
                cont = Cont::Slot { node: idx, slot: SlotRef(0) };
            }
        }
    }
}

pub(crate) fn execute_task<N, H, R, F, G, P: FunnelPolicy>(
    wctx: &WorkerCtx<N, H, R, F, G, P>,
    task: FunnelTask<N, H, R>,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    match task {
        FunnelTask::Walk { child, cont } => walk_cps(wctx, child, cont),
    }
}
