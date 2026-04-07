//! Accumulation strategy axis.
//!
//! Controls HOW child results are accumulated into the parent's heap:
//! - OnArrival: streaming sweep with CAS gate. Accumulate as results arrive.
//! - OnFinalize: bulk sweep by the last event. Accumulate all at once.

pub mod on_arrival;
pub mod on_finalize;

pub use on_arrival::OnArrival;
pub use on_finalize::OnFinalize;

use crate::ops::FoldOps;
use super::super::chain::{FoldChain, SlotRef};

/// Accumulation strategy: how child results flow into the parent's heap.
pub trait AccumulateStrategy: 'static {
    fn deliver<N, H, R>(
        chain: &FoldChain<H, R>, slot: SlotRef, result: R,
        fold: &impl FoldOps<N, H, R>,
    ) -> Option<R>;

    fn set_total<N, H, R>(
        chain: &FoldChain<H, R>,
        fold: &impl FoldOps<N, H, R>,
    ) -> Option<R>;
}
