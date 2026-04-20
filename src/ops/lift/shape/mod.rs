// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Universal `ShapeLift` struct + polymorphic `Lift<D, …>` impl.
//!
//! Concrete shape-lifts (wrap_init, map_r, explainer, contramap_n,
//! inline, …) are constructor functions on the capable domain
//! types — see `domain/shared/shape_lifts.rs` and
//! `domain/local/shape_lifts.rs`.

pub mod universal;

pub use universal::ShapeLift;
