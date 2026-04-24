//! CPS data types: defunctionalized tasks and continuations.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};
use super::chain::{FoldChain, SlotRef};
use crate::exec::funnel::infra::arena::ArenaIdx;
use crate::exec::funnel::infra::cont_arena::ContIdx;

// ANCHOR: funnel_task
pub enum FunnelTask<N, H, R> {
    Walk { child: N, cont: Cont<H, R> },
}
// ANCHOR_END: funnel_task

unsafe impl<N: Send, H, R: Send> Send for FunnelTask<N, H, R> {}

// ANCHOR: cont_enum
pub enum Cont<H, R> {
    /// Raw pointer to stack-local RootCell in run_fold.
    /// SAFETY: The scoped pool guarantees all workers complete before
    /// run_fold returns — the RootCell outlives every Cont::Root.
    Root(*const RootCell<R>),
    Direct { heap: H, parent_idx: ContIdx },
    Slot { node: ArenaIdx, slot: SlotRef },
}
// ANCHOR_END: cont_enum

unsafe impl<H, R: Send> Send for Cont<H, R> {}

// ANCHOR: chain_node
pub(crate) struct ChainNode<H, R> {
    pub(crate) chain: FoldChain<H, R>,
    parent_cont: UnsafeCell<Option<Cont<H, R>>>,
}
// ANCHOR_END: chain_node

unsafe impl<H, R: Send> Send for ChainNode<H, R> {}
unsafe impl<H, R: Send> Sync for ChainNode<H, R> {}

impl<H, R> ChainNode<H, R> {
    pub(crate) fn new(heap: H, cont: Cont<H, R>) -> Self {
        ChainNode { chain: FoldChain::new(heap), parent_cont: UnsafeCell::new(Some(cont)) }
    }
    pub(crate) fn take_parent_cont(&self) -> Cont<H, R> {
        // SAFETY: take_parent_cont is called exactly once per ChainNode
        // — from finalize_chain at the node's finalization point. No
        // other worker touches parent_cont after construction.
        unsafe { (*self.parent_cont.get()).take().expect("parent cont already taken") }
    }
}

// ── Root result cell ──────────────────────────────────

pub struct RootCell<R> {
    result: UnsafeCell<Option<R>>,
    done: AtomicBool,
}

unsafe impl<R: Send> Send for RootCell<R> {}
unsafe impl<R: Send> Sync for RootCell<R> {}

impl<R> RootCell<R> {
    pub(crate) fn new() -> Self { RootCell { result: UnsafeCell::new(None), done: AtomicBool::new(false) } }
    pub(crate) fn set(&self, r: R) {
        // SAFETY: set is called exactly once — from the unique
        // finalize that produces the root result. The subsequent
        // Release store on `done` publishes the write to whoever
        // observes `is_done()` via Acquire.
        unsafe { *self.result.get() = Some(r); }
        self.done.store(true, Ordering::Release);
    }
    pub(crate) fn is_done(&self) -> bool { self.done.load(Ordering::Acquire) }
    pub(crate) fn take(&self) -> R {
        // SAFETY: only called by run_fold after observing done=true
        // (Acquire), which happens-after the write in `set`.
        unsafe { (*self.result.get()).take().expect("root result not set") }
    }
}
