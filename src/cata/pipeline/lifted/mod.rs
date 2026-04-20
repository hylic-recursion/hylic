//! LiftedPipeline — Stage 2 of the pipeline typestate.
//!
//! Parametric over its source: `LiftedPipeline<Base, L>` wraps any
//! `Base: PipelineSource` (SeedPipeline, TreeishPipeline, or any
//! future source) with a lift chain `L`. Sole Stage-2 primitive:
//! `apply_pre_lift`.

use crate::ops::IdentityLift;

pub mod apply_pre_lift;
pub mod transforms_shared;
pub mod transforms_local;
pub mod source_impl;

pub struct LiftedPipeline<Base, L = IdentityLift> {
    pub(crate) base:     Base,
    pub(crate) pre_lift: L,
}

impl<Base, L> LiftedPipeline<Base, L> {
    pub(crate) fn new(base: Base, pre_lift: L) -> Self {
        LiftedPipeline { base, pre_lift }
    }
}

impl<Base: Clone, L: Clone> Clone for LiftedPipeline<Base, L> {
    fn clone(&self) -> Self {
        LiftedPipeline {
            base:     self.base.clone(),
            pre_lift: self.pre_lift.clone(),
        }
    }
}
