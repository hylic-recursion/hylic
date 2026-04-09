//! EveryK: notify every K-th push. K=1 = EveryPush. K=∞ ≈ OncePerBatch.
//! K is a const generic — the modulus compiles to a bitmask when K is a power of 2.

use super::WakeStrategy;

pub struct EveryK<const K: u32>;

#[derive(Clone, Copy, Default)]
pub struct EveryKSpec;
unsafe impl Send for EveryKSpec {}
unsafe impl Sync for EveryKSpec {}

impl<const K: u32> WakeStrategy for EveryK<K> {
    type Spec = EveryKSpec;
    type State = u32;

    fn init_state(_spec: &EveryKSpec) -> u32 { 0 }

    fn should_notify(count: &mut u32) -> bool {
        *count += 1;
        *count == 1 || *count % K == 0
    }

    fn reset(count: &mut u32) { *count = 0; }
}
