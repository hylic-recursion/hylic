//! Wake strategy axis.
//!
//! Controls HOW workers are notified of new tasks:
//! - EveryPush: notify on every push (robust default)
//! - OncePerBatch: notify first push per visit only (noop optimization)
//! - EveryK: notify every K-th push (tunable)

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

pub mod every_push;
pub mod once_per_batch;
pub mod every_k;

pub use every_push::EveryPush;
pub use once_per_batch::OncePerBatch;
pub use every_k::EveryK;

// ANCHOR: wake_strategy_trait
/// Wake strategy: when to notify idle workers of pushed tasks.
///
/// `State` is per-worker mutable state (embedded in WorkerCtx as
/// `Cell<State>`). Created once via `init_state`, reset per visit batch.
pub trait WakeStrategy: 'static {
    type Spec: Copy + Default + Send + Sync;
    type State: Copy;

    fn init_state(spec: &Self::Spec) -> Self::State;

    /// Called after each successful push.
    /// Returns true if the caller should wake an idle worker.
    fn should_notify(state: &mut Self::State) -> bool;

    /// Called before each graph.visit batch.
    fn reset(state: &mut Self::State);
}
// ANCHOR_END: wake_strategy_trait
