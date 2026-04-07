//! FunnelPolicy: bundles the three behavioral axes into one type parameter.
//!
//! Pre-defined policies for common workloads. Custom policies via trait impl.
//!
//! Usage: `HyloFunnelIn::<domain::Shared, policy::Default>::new(nw, spec)`

use super::queue::{self, WorkStealing};
use super::accumulate::{self, AccumulateStrategy};
use super::wake::{self, WakeStrategy};

/// Bundles queue topology, accumulation strategy, and wake policy.
/// One type parameter on the executor replaces three.
pub trait FunnelPolicy: 'static {
    type Queue: WorkStealing;
    type Accumulate: AccumulateStrategy;
    type Wake: WakeStrategy;
}

/// General-purpose default. No regressions on any workload.
/// PerWorker + OnFinalize + EveryPush.
pub struct Default;
impl FunnelPolicy for Default {
    type Queue = queue::PerWorker;
    type Accumulate = accumulate::OnFinalize;
    type Wake = wake::EveryPush;
}

/// Heavy graph traversal, light accumulate (module resolution).
/// PerWorker + OnFinalize + EveryPush.
pub struct GraphHeavy;
impl FunnelPolicy for GraphHeavy {
    type Queue = queue::PerWorker;
    type Accumulate = accumulate::OnFinalize;
    type Wake = wake::EveryPush;
}

/// Wide trees (bf=20+), light per-node work.
/// Shared + OnArrival + EveryPush.
pub struct WideLight;
impl FunnelPolicy for WideLight {
    type Queue = queue::Shared;
    type Accumulate = accumulate::OnArrival;
    type Wake = wake::EveryPush;
}

/// Overhead-sensitive: noop-like, many small folds.
/// PerWorker + OnFinalize + OncePerBatch.
pub struct LowOverhead;
impl FunnelPolicy for LowOverhead {
    type Queue = queue::PerWorker;
    type Accumulate = accumulate::OnFinalize;
    type Wake = wake::OncePerBatch;
}

/// PerWorker + OnArrival + EveryPush. Streaming sweep with per-worker deques.
pub struct PerWorkerArrival;
impl FunnelPolicy for PerWorkerArrival {
    type Queue = queue::PerWorker;
    type Accumulate = accumulate::OnArrival;
    type Wake = wake::EveryPush;
}

/// Shared + OnFinalize + EveryPush.
pub struct SharedDefault;
impl FunnelPolicy for SharedDefault {
    type Queue = queue::Shared;
    type Accumulate = accumulate::OnFinalize;
    type Wake = wake::EveryPush;
}
