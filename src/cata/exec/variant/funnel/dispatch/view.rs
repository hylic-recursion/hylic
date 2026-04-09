//! FoldView: per-fold shared state.
//! Borrows PoolState (scoped, not Arc). Owns per-fold atomics.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::super::infra::eventcount::EventCount;
use super::super::pool::PoolState;

pub(crate) struct FoldView<'a> {
    pub pool_state: &'a PoolState,
    pub fold_done: AtomicBool,
    pub idle_count: AtomicU32,
    pub n_workers: usize,
}

impl FoldView<'_> {
    pub fn event(&self) -> &EventCount { &self.pool_state.wake }

    pub fn notify_idle(&self) {
        if self.idle_count.load(Ordering::Relaxed) > 0 {
            self.pool_state.wake.notify_one();
        }
    }
}
