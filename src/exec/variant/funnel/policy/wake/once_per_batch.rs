//! OncePerBatch: notify only on the first push of each visit batch.
//! Reduces redundant wakeups. Best for noop/overhead-sensitive.
//! Regresses on wide trees with light work (workers starve).

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use super::WakeStrategy;

pub struct OncePerBatch;

#[derive(Clone, Copy, Default)]
pub struct OncePerBatchSpec;
unsafe impl Send for OncePerBatchSpec {}
unsafe impl Sync for OncePerBatchSpec {}

impl WakeStrategy for OncePerBatch {
    type Spec = OncePerBatchSpec;
    type State = bool;

    fn init_state(_spec: &OncePerBatchSpec) -> bool { false }

    fn should_notify(notified: &mut bool) -> bool {
        if *notified { return false; }
        *notified = true;
        true
    }

    fn reset(notified: &mut bool) { *notified = false; }
}
