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
use super::super::dispatch::view::FoldView;
use super::super::dispatch::worker::WorkerCtx;
use super::super::policy::FunnelPolicy;
use super::super::policy::accumulate::AccumulateStrategy;
use super::chain::SlotRef;
use super::super::infra::arena::Arena;
use super::super::infra::cont_arena::ContArena;

// ── Shared immutable context (created once, passed by &ref) ──
// All references. No raw pointers. Lifetime 'a = run_fold's stack frame.

pub(crate) struct WalkCtx<'a, F, G, H, R, P: FunnelPolicy> {
    pub(crate) fold: &'a F,
    pub(crate) graph: &'a G,
    pub(crate) view: &'a FoldView<'a>,
    pub(crate) chain_arena: &'a Arena<ChainNode<H, R>>,
    pub(crate) cont_arena: &'a ContArena<Cont<H, R>>,
    pub(crate) _policy: std::marker::PhantomData<P>,
}

// SAFETY: All referenced data lives for the scoped pool duration.
// Workers access WalkCtx through FoldState, which is erased to *const ()
// at the Job boundary. The scoped pool guarantees the data outlives all workers.
unsafe impl<F: Sync, G: Sync, H, R: Send, P: FunnelPolicy> Send for WalkCtx<'_, F, G, H, R, P> {}
unsafe impl<F: Sync, G: Sync, H, R: Send, P: FunnelPolicy> Sync for WalkCtx<'_, F, G, H, R, P> {}

impl<'a, F, G, H, R, P: FunnelPolicy> WalkCtx<'a, F, G, H, R, P> {
    pub(crate) fn fold_ref(&self) -> &'a F { self.fold }
    pub(crate) fn graph_ref(&self) -> &'a G { self.graph }
    pub(crate) fn view_ref(&self) -> &'a FoldView<'a> { self.view }
    pub(crate) fn chain_arena(&self) -> &'a Arena<ChainNode<H, R>> { self.chain_arena }
    pub(crate) fn cont_arena(&self) -> &'a ContArena<Cont<H, R>> { self.cont_arena }
}

// ANCHOR: fire_cont
pub(crate) fn fire_cont<N, H, R, F, G, P: FunnelPolicy>(
    ctx: &WalkCtx<'_, F, G, H, R, P>,
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
                let view = ctx.view_ref();
                view.fold_done.store(true, Ordering::Release);
                view.event().notify_all();
                return;
            }
            Cont::Direct { mut heap, parent_idx } => {
                let fold = ctx.fold_ref();
                fold.accumulate(&mut heap, &result);
                result = fold.finalize(&heap);
                cont = unsafe { ctx.cont_arena().take(parent_idx) };
            }
            Cont::Slot { node: node_idx, slot } => {
                let arena = ctx.chain_arena();
                let node = unsafe { arena.get(node_idx) };
                let fold = ctx.fold_ref();
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
// ANCHOR_END: fire_cont

// ANCHOR: walk_cps
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
        let fold = ctx.fold_ref();
        let graph = ctx.graph_ref();
        let chain_arena = ctx.chain_arena();
        let cont_arena = ctx.cont_arena();
        let heap = fold.init(&node);

        let mut child_count = 0u32;
        let mut first_child: Option<N> = None;
        let mut chain_idx: Option<super::super::infra::arena::ArenaIdx> = None;
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
                let fold = ctx.fold_ref();
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
// ANCHOR_END: walk_cps

// ANCHOR: execute_task
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
// ANCHOR_END: execute_task
