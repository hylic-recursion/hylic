//! OnArrival: streaming sweep with CAS gate.
//!
//! Each delivery tries to sweep contiguous filled slots. The CAS gate
//! ensures only one thread sweeps at a time. Results accumulate as they
//! arrive — earlier cascade start, but CAS contention when accumulate
//! is heavy.

use crate::ops::FoldOps;
use super::super::super::chain::{FoldChain, SlotRef};
use super::AccumulateStrategy;

pub struct OnArrival;

impl AccumulateStrategy for OnArrival {
    fn deliver<N, H, R>(
        chain: &FoldChain<H, R>, slot: SlotRef, result: R,
        fold: &impl FoldOps<N, H, R>,
    ) -> Option<R> {
        chain.deliver_and_sweep(slot, result, fold)
    }

    fn set_total<N, H, R>(
        chain: &FoldChain<H, R>,
        fold: &impl FoldOps<N, H, R>,
    ) -> Option<R> {
        chain.set_total_and_sweep(fold)
    }
}
