//! FunnelPolicy: bundles the three behavioral axes into one type parameter.
//!
//! `Policy<Q, A, W>` is the generic implementor. Named presets are type aliases.
//! Custom policies: use `Policy<MyQueue, MyAccumulate, MyWake>` or impl FunnelPolicy.

pub mod queue;
pub mod accumulate;
pub mod wake;

use std::marker::PhantomData;
use queue::WorkStealing;
use accumulate::AccumulateStrategy;
use wake::WakeStrategy;

// ANCHOR: funnel_policy_trait
/// Bundles queue topology, accumulation strategy, and wake policy.
/// One type parameter on the executor replaces three.
pub trait FunnelPolicy: 'static {
    type Queue: WorkStealing;
    type Accumulate: AccumulateStrategy;
    type Wake: WakeStrategy;
}
// ANCHOR_END: funnel_policy_trait

// ANCHOR: policy_struct
/// Generic policy: any combination of axes. Named presets are type aliases over this.
pub struct Policy<
    Q: WorkStealing = queue::PerWorker,
    A: AccumulateStrategy = accumulate::OnFinalize,
    W: WakeStrategy = wake::EveryPush,
>(PhantomData<(Q, A, W)>);

impl<Q: WorkStealing, A: AccumulateStrategy, W: WakeStrategy> FunnelPolicy for Policy<Q, A, W> {
    type Queue = Q;
    type Accumulate = A;
    type Wake = W;
}
// ANCHOR_END: policy_struct

// ANCHOR: named_presets
// ── Named presets (type aliases) ─────────────────

/// Robust all-rounder. PerWorker + OnFinalize + EveryPush.
pub type Robust = Policy;

/// The general-purpose default policy IS Robust.
pub type Default = Robust;

/// Same axes as Robust — distinguished by Spec constructor (larger arenas).
pub type GraphHeavy = Robust;

/// Wide trees (bf=20+). Shared + OnArrival + EveryPush.
pub type WideLight = Policy<queue::Shared, accumulate::OnArrival>;

/// Overhead-sensitive (noop-like). PerWorker + OnFinalize + OncePerBatch.
pub type LowOverhead = Policy<queue::PerWorker, accumulate::OnFinalize, wake::OncePerBatch>;

/// PerWorker + OnArrival + EveryPush. Streaming sweep with per-worker deques.
pub type PerWorkerArrival = Policy<queue::PerWorker, accumulate::OnArrival>;

/// Shared + OnFinalize + EveryPush.
pub type SharedDefault = Policy<queue::Shared>;

/// PerWorker + OnFinalize + EveryK<4>. Balanced wakeups for heavy workloads.
pub type HighThroughput = Policy<queue::PerWorker, accumulate::OnFinalize, wake::EveryK<4>>;

/// Shared + OnArrival + OncePerBatch.
pub type StreamingWide = Policy<queue::Shared, accumulate::OnArrival, wake::OncePerBatch>;

/// PerWorker + OnFinalize + EveryK<2>. For deep narrow trees (bf=2).
pub type DeepNarrow = Policy<queue::PerWorker, accumulate::OnFinalize, wake::EveryK<2>>;
// ANCHOR_END: named_presets

// ── Typestate axis builders ──────────────────────
// Only available when P = Policy<Q, A, W> (not custom FunnelPolicy impls).

impl<Q: WorkStealing, A: AccumulateStrategy, W: WakeStrategy>
    super::Spec<Policy<Q, A, W>>
{
    /// Change queue strategy. Provide the new queue's Spec.
    pub fn with_queue<Q2: WorkStealing>(self, queue_spec: Q2::Spec) -> super::Spec<Policy<Q2, A, W>> {
        super::Spec {
            n_workers: self.n_workers,
            chain_arena_capacity: self.chain_arena_capacity,
            cont_arena_capacity: self.cont_arena_capacity,
            queue: queue_spec, accumulate: self.accumulate, wake: self.wake,
        }
    }

    /// Change accumulate strategy. Provide the new accumulate's Spec.
    pub fn with_accumulate<A2: AccumulateStrategy>(self, acc_spec: A2::Spec) -> super::Spec<Policy<Q, A2, W>> {
        super::Spec {
            n_workers: self.n_workers,
            chain_arena_capacity: self.chain_arena_capacity,
            cont_arena_capacity: self.cont_arena_capacity,
            queue: self.queue, accumulate: acc_spec, wake: self.wake,
        }
    }

    /// Change wake strategy. Provide the new wake's Spec.
    pub fn with_wake<W2: WakeStrategy>(self, wake_spec: W2::Spec) -> super::Spec<Policy<Q, A, W2>> {
        super::Spec {
            n_workers: self.n_workers,
            chain_arena_capacity: self.chain_arena_capacity,
            cont_arena_capacity: self.cont_arena_capacity,
            queue: self.queue, accumulate: self.accumulate, wake: wake_spec,
        }
    }
}
