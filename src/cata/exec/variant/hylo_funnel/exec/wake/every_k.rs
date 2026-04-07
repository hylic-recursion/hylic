//! EveryK: notify every K-th push. K=1 = EveryPush. K=∞ ≈ OncePerBatch.
//! Tunable middle ground for wide trees.

use super::WakeStrategy;

pub struct EveryK;

#[derive(Clone)]
pub struct EveryKSpec {
    pub k: u32,
}

impl Default for EveryKSpec {
    fn default() -> Self { EveryKSpec { k: 4 } }
}

unsafe impl Send for EveryKSpec {}
unsafe impl Sync for EveryKSpec {}

/// State carries K (from spec) + push counter. Both fit in one u64
/// which is Copy. Counter resets per batch; K persists.
#[derive(Clone, Copy)]
pub struct EveryKState {
    k: u32,
    count: u32,
}

impl WakeStrategy for EveryK {
    type Spec = EveryKSpec;
    type State = EveryKState;

    fn init_state(spec: &EveryKSpec) -> EveryKState {
        EveryKState { k: spec.k.max(1), count: 0 }
    }

    fn should_notify(state: &mut EveryKState, _idle_count: u32) -> bool {
        state.count += 1;
        state.count == 1 || state.count % state.k == 0
    }

    fn reset(state: &mut EveryKState) { state.count = 0; }
}
