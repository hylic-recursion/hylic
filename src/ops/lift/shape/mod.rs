//! Universal `ShapeLift` struct + polymorphic `Lift<D, …>` impl.
//!
//! Concrete shape-lifts (wrap_init, map_r, explainer, contramap_n,
//! inline, …) are constructor functions on the capable domain
//! types — see `domain/shared/shape_lifts.rs` and
//! `domain/local/shape_lifts.rs`.

pub mod universal;

pub use universal::ShapeLift;
