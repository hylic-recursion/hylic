//! CPS walk: defunctionalized tasks, packed-ticket streaming sweep.
//!
//! walk_cps is void — delivers results through defunctionalized continuations.
//! fire_cont calls deliver_and_sweep inline.
//! fold_done set inside fire_cont(Cont::Root) — CPS completion signal.

use std::sync::atomic::Ordering;
use crate::ops::{FoldOps, TreeOps};
use super::cont::{FunnelTask, Cont, ChainNode};
use super::view::FoldView;
use super::worker::WorkerCtx;
use super::fold_chain::SlotRef;
use super::arena::Arena;
use super::cont_arena::ContArena;
use super::AccumulateMode;

// ── Shared immutable context (created once, passed by &ref) ──

pub(super) struct WalkCtx<F, G, H, R> {
    pub(super) fold: *const F,
    pub(super) graph: *const G,
    pub(super) view: *const FoldView,
    pub(super) chain_arena: *const Arena<ChainNode<H, R>>,
    pub(super) cont_arena: *const ContArena<Cont<H, R>>,
    pub(super) accumulate: AccumulateMode,
}

unsafe impl<F, G, H, R> Send for WalkCtx<F, G, H, R> {}
unsafe impl<F, G, H, R> Sync for WalkCtx<F, G, H, R> {}

impl<F, G, H, R> WalkCtx<F, G, H, R> {
    pub(super) unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }
    pub(super) unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
    pub(super) unsafe fn view_ref(&self) -> &FoldView { unsafe { &*self.view } }
    pub(super) unsafe fn chain_arena(&self) -> &Arena<ChainNode<H, R>> { unsafe { &*self.chain_arena } }
    pub(super) unsafe fn cont_arena(&self) -> &ContArena<Cont<H, R>> { unsafe { &*self.cont_arena } }
}

// ── fire_cont (trampolined) ──────────────────────────

pub(super) fn fire_cont<N, H, R, F, G>(
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
                let delivered = match ctx.accumulate {
                    AccumulateMode::OnArrival => node.chain.deliver_and_sweep(slot, result, fold),
                    AccumulateMode::OnFinalize => node.chain.deliver_and_finalize(slot, result, fold),
                };
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

pub(super) fn walk_cps<N, H, R, F, G>(
    wctx: &WorkerCtx<N, H, R, F, G>,
    node: N,
    cont: Cont<H, R>,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let ctx = wctx.ctx;
    let fold = unsafe { ctx.fold_ref() };
    let graph = unsafe { ctx.graph_ref() };
    let chain_arena = unsafe { ctx.chain_arena() };
    let cont_arena = unsafe { ctx.cont_arena() };
    let heap = fold.init(&node);

    let mut child_count = 0u32;
    let mut first_child: Option<N> = None;
    let mut chain_idx: Option<super::arena::ArenaIdx> = None;
    let mut heap_opt = Some(heap);
    let mut cont_opt = Some(cont);

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
            fire_cont::<N, H, R, F, G>(ctx, cont, result);
        }
        1 => {
            let child = first_child.unwrap();
            let heap = heap_opt.take().unwrap();
            let parent_cont = cont_opt.take().unwrap();
            let parent_idx = cont_arena.alloc(parent_cont);
            walk_cps(wctx, child, Cont::Direct { heap, parent_idx });
        }
        _ => {
            let idx = chain_idx.unwrap();
            let cn = unsafe { chain_arena.get(idx) };
            let fold = unsafe { ctx.fold_ref() };
            let set_total_result = match ctx.accumulate {
                AccumulateMode::OnArrival => cn.chain.set_total_and_sweep(fold),
                AccumulateMode::OnFinalize => cn.chain.set_total_and_finalize(fold),
            };
            if let Some(finalized) = set_total_result {
                let parent = cn.take_parent_cont();
                fire_cont::<N, H, R, F, G>(ctx, parent, finalized);
                return;
            }
            let child = first_child.unwrap();
            walk_cps(wctx, child, Cont::Slot {
                node: idx, slot: SlotRef(0),
            });
        }
    }
}

pub(super) fn execute_task<N, H, R, F, G>(
    wctx: &WorkerCtx<N, H, R, F, G>,
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
