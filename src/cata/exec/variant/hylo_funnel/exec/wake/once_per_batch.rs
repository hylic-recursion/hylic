//! OncePerBatch: notify only on the first push of each visit batch.
//! Reduces redundant wakeups. Best for noop/overhead-sensitive.
//! Regresses on wide trees with light work (workers starve).

use super::WakeStrategy;

pub struct OncePerBatch;

#[derive(Clone, Default)]
pub struct OncePerBatchSpec;
unsafe impl Send for OncePerBatchSpec {}
unsafe impl Sync for OncePerBatchSpec {}

impl WakeStrategy for OncePerBatch {
    type Spec = OncePerBatchSpec;
    type State = bool;

    fn init_state(_spec: &OncePerBatchSpec) -> bool { false }

    fn should_notify(notified: &mut bool, _idle_count: u32) -> bool {
        if *notified { return false; }
        *notified = true;
        true
    }

    fn reset(notified: &mut bool) { *notified = false; }
}
