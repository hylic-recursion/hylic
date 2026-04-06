//! FoldView: per-fold view into the pool + work-stealing helper.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use super::super::deque::WorkerDeque;
use super::super::eventcount::EventCount;
use super::super::pool::PoolInner;

// ── FoldView (per-fold, stack-local, executor-owned) ─

pub(crate) struct FoldView {
    pub pool_inner: Arc<PoolInner>,
    pub fold_done: AtomicBool,
    pub idle_count: AtomicU32,
    pub work_available: AtomicU64,
    pub n_workers: usize,
}

impl FoldView {
    pub fn event(&self) -> &EventCount { &self.pool_inner.wake }

    /// Signal that deque `idx` has work. Call after pushing a task.
    pub fn signal_push(&self, deque_idx: usize) {
        self.work_available.fetch_or(1u64 << deque_idx, Ordering::Relaxed);
        if self.idle_count.load(Ordering::Relaxed) > 0 {
            self.pool_inner.wake.notify_one();
        }
    }
}

// ── Steal helper ─────────────────────────────────────

pub(crate) fn steal_from_others<T>(
    deques: &[WorkerDeque<T>],
    my_idx: usize,
    view: &FoldView,
) -> Option<T> {
    let mut bits = view.work_available.load(Ordering::Relaxed);
    bits &= !(1u64 << my_idx); // don't steal from self
    while bits != 0 {
        let target = bits.trailing_zeros() as usize;
        if let Some(task) = deques[target].steal() {
            return Some(task);
        }
        // Deque was empty — clear its bit and try next
        view.work_available.fetch_and(!(1u64 << target), Ordering::Relaxed);
        bits &= !(1u64 << target);
    }
    None
}
