//! EveryPush: notify on every successful push.
//! Robust default. No missed wakes. No dedup overhead.

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use super::WakeStrategy;

pub struct EveryPush;

#[derive(Clone, Copy, Default)]
pub struct EveryPushSpec;
// SAFETY: unit spec with no interior mutability.
unsafe impl Send for EveryPushSpec {}
unsafe impl Sync for EveryPushSpec {}

impl WakeStrategy for EveryPush {
    type Spec = EveryPushSpec;
    type State = ();

    fn init_state(_spec: &EveryPushSpec) -> () {}
    fn should_notify(_state: &mut ()) -> bool { true }
    fn reset(_state: &mut ()) {}
}
