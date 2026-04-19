//! Explorative tests — self-contained designs being probed.
//!
//! Phase 3 note: the pre-CPS modules (map_constituents, bifunctor_lift,
//! type_morphing_prelift, lift_bounds) target the old Lift<N, N2>
//! bifunctor trait and will not compile until they are migrated (or
//! superseded). Disabled during the CPS refactor. `concrete_seed_lift`
//! is the live experiment proving the Seed-at-trait-level solution.

// Old-design explorative tests — compile against the pre-CPS Lift
// trait. Disabled during Phase 3; they'll be superseded by the
// power_user/ suite once the refactor completes.
//
// mod map_constituents;
// mod bifunctor_lift;
// mod type_morphing_prelift;
// mod lift_bounds;

// The live Phase-3 experiment.
mod concrete_seed_lift;
