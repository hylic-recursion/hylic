//! LiftedPipeline — Stage 2 of the Phase-3 typestate. Holds its
//! Stage-1 ancestor as `base` plus a lift chain `pre_lift: L`.
//! Sole primitive: apply_pre_lift.

use crate::ops::IdentityLift;
use super::seed::SeedPipeline;

pub mod apply_pre_lift;
pub mod transforms;
pub mod source_impl;

pub struct LiftedPipeline<N, Seed, H, R, L = IdentityLift> {
    pub(crate) base:     SeedPipeline<N, Seed, H, R>,
    pub(crate) pre_lift: L,
}

impl<N, Seed, H, R, L> LiftedPipeline<N, Seed, H, R, L> {
    pub(crate) fn new(base: SeedPipeline<N, Seed, H, R>, pre_lift: L) -> Self {
        LiftedPipeline { base, pre_lift }
    }
}

impl<N, Seed, H, R, L: Clone> Clone for LiftedPipeline<N, Seed, H, R, L> {
    fn clone(&self) -> Self {
        LiftedPipeline {
            base:     self.base.clone(),
            pre_lift: self.pre_lift.clone(),
        }
    }
}
