//! CPS data types: defunctionalized tasks and continuations.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use super::fold_chain::{FoldChain, SlotRef};
use super::arena::ArenaIdx;
use super::cont_arena::ContIdx;

// ── Defunctionalized task ────────────────────────────

pub(super) enum FunnelTask<N, H, R> {
    Walk { child: N, cont: Cont<H, R> },
}

unsafe impl<N: Send, H, R: Send> Send for FunnelTask<N, H, R> {}

// ── Defunctionalized continuation ─────────────────────

pub(super) enum Cont<H, R> {
    Root(Arc<RootCell<R>>),
    Direct { heap: H, parent_idx: ContIdx },
    Slot { node: ArenaIdx, slot: SlotRef },
}

unsafe impl<H, R: Send> Send for Cont<H, R> {}

// ── ChainNode (multi-child only, arena-allocated) ────

pub(super) struct ChainNode<H, R> {
    pub(super) chain: FoldChain<H, R>,
    parent_cont: UnsafeCell<Option<Cont<H, R>>>,
}

unsafe impl<H, R: Send> Send for ChainNode<H, R> {}
unsafe impl<H, R: Send> Sync for ChainNode<H, R> {}

impl<H, R> ChainNode<H, R> {
    pub(super) fn new(heap: H, cont: Cont<H, R>) -> Self {
        ChainNode { chain: FoldChain::new(heap), parent_cont: UnsafeCell::new(Some(cont)) }
    }
    pub(super) fn take_parent_cont(&self) -> Cont<H, R> {
        unsafe { (*self.parent_cont.get()).take().expect("parent cont already taken") }
    }
}

// ── Root result cell ──────────────────────────────────

pub(super) struct RootCell<R> {
    result: UnsafeCell<Option<R>>,
    done: AtomicBool,
}

unsafe impl<R: Send> Send for RootCell<R> {}
unsafe impl<R: Send> Sync for RootCell<R> {}

impl<R> RootCell<R> {
    pub(super) fn new() -> Self { RootCell { result: UnsafeCell::new(None), done: AtomicBool::new(false) } }
    pub(super) fn set(&self, r: R) {
        unsafe { *self.result.get() = Some(r); }
        self.done.store(true, Ordering::Release);
    }
    pub(super) fn is_done(&self) -> bool { self.done.load(Ordering::Acquire) }
    pub(super) fn take(&self) -> R { unsafe { (*self.result.get()).take().expect("root result not set") } }
}
