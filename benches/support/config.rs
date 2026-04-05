//! Benchmark config IDs — single source of truth.
//!
//! Convention: `hylic.{lift?}.{executor}.{domain}`
//! Baselines:  `{origin}.{strategy}`
//!
//! Both modes.rs and module_sim.rs reference these constants.
//! Adding a new config = add one const here + one closure there.

// ── Sequential (no parallelism) ──────────────────

pub const FUSED_SHARED:      &str = "hylic.fused.shared";
pub const FUSED_LOCAL:       &str = "hylic.fused.local";
pub const FUSED_OWNED:       &str = "hylic.fused.owned";
pub const SEQUENTIAL_SHARED: &str = "hylic.sequential.shared";
pub const SEQUENTIAL_LOCAL:  &str = "hylic.sequential.local";
pub const SEQUENTIAL_OWNED:  &str = "hylic.sequential.owned";

// ── Parallel (direct executors) ──────────────────

pub const RAYON_SHARED:         &str = "hylic.rayon.shared";
pub const POOL_SHARED:          &str = "hylic.pool.shared";
pub const POOL_LOCAL:           &str = "hylic.pool.local";
pub const POOL_OWNED:           &str = "hylic.pool.owned";
pub const HYLO_SHARED:          &str = "hylic.hylo.shared";
pub const FUNNEL_SHARED:        &str = "hylic.funnel.shared";

// ── Parallel (with lifts — Shared) ───────────────

pub const PARREF_FUSED_SHARED:  &str = "hylic.parref.fused.shared";
pub const PARREF_RAYON_SHARED:  &str = "hylic.parref.rayon.shared";
pub const PARREF_POOL_SHARED:   &str = "hylic.parref.pool.shared";
pub const EAGER_FUSED_SHARED:   &str = "hylic.eager.fused.shared";
pub const EAGER_RAYON_SHARED:   &str = "hylic.eager.rayon.shared";
pub const EAGER_POOL_SHARED:    &str = "hylic.eager.pool.shared";

// ── Parallel (with lifts — Local) ────────────────

pub const PARREF_FUSED_LOCAL:   &str = "hylic.parref.fused.local";
pub const PARREF_POOL_LOCAL:    &str = "hylic.parref.pool.local";
pub const EAGER_FUSED_LOCAL:    &str = "hylic.eager.fused.local";
pub const EAGER_POOL_LOCAL:     &str = "hylic.eager.pool.local";

// ── Thread count (match rayon's default) ────────

/// Number of worker threads for all pool-based benchmarks.
/// Reads rayon's actual thread count for exact fairness.
pub fn bench_workers() -> usize {
    rayon::current_num_threads()
}

// ── Baselines ────────────────────────────────────

pub const HAND_SEQ:      &str = "hand.seq";
pub const HAND_RAYON:    &str = "hand.rayon";
pub const HAND_POOL:     &str = "hand.pool";
pub const REAL_SEQ:      &str = "real.seq";
pub const REAL_RAYON:    &str = "real.rayon";
pub const VANILLA_SEQ:   &str = "vanilla.seq";
pub const VANILLA_RAYON: &str = "vanilla.rayon";
