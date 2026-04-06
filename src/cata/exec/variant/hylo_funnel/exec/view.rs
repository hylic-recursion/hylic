//! FoldView: per-fold shared state (fold_done, idle_count, eventcount).
//! Queue-agnostic — no bitmask, no deque references.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use super::super::eventcount::EventCount;
use super::super::pool::PoolInner;

pub(crate) struct FoldView {
    pub pool_inner: Arc<PoolInner>,
    pub fold_done: AtomicBool,
    pub idle_count: AtomicU32,
    pub n_workers: usize,
}

impl FoldView {
    pub fn event(&self) -> &EventCount { &self.pool_inner.wake }

    pub fn notify_idle(&self) {
        if self.idle_count.load(Ordering::Relaxed) > 0 {
            self.pool_inner.wake.notify_one();
        }
    }
}
