//! OnArrival: streaming sweep with CAS gate.
//!
//! Each delivery tries to sweep contiguous filled slots. The CAS gate
//! ensures only one thread sweeps at a time. Results accumulate as they
//! arrive — earlier cascade start, but CAS contention when accumulate
//! is heavy.

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use crate::ops::FoldOps;
use crate::exec::funnel::cps::chain::{FoldChain, SlotRef};
use super::AccumulateStrategy;

pub struct OnArrival;

#[derive(Clone, Copy, Default)]
pub struct OnArrivalSpec;
unsafe impl Send for OnArrivalSpec {}
unsafe impl Sync for OnArrivalSpec {}

impl AccumulateStrategy for OnArrival {
    type Spec = OnArrivalSpec;

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
