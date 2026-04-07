//! OnFinalize: bulk sweep by the last event.
//!
//! Deliveries store + ticket only. No CAS gate, no contention.
//! The last event bulk-sweeps all slots and accumulates sequentially.
//! Lower per-delivery overhead. No CAS contention on heavy accumulate.

use crate::ops::FoldOps;
use super::super::super::chain::{FoldChain, SlotRef};
use super::AccumulateStrategy;

pub struct OnFinalize;

impl AccumulateStrategy for OnFinalize {
    fn deliver<N, H, R>(
        chain: &FoldChain<H, R>, slot: SlotRef, result: R,
        fold: &impl FoldOps<N, H, R>,
    ) -> Option<R> {
        chain.deliver_and_finalize(slot, result, fold)
    }

    fn set_total<N, H, R>(
        chain: &FoldChain<H, R>,
        fold: &impl FoldOps<N, H, R>,
    ) -> Option<R> {
        chain.set_total_and_finalize(fold)
    }
}
