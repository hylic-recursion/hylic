//! Lift trait + library lifts.
//!
//! `Lift<D, N, H, R>` (in `core`) is the domain-generic triple
//! transformer trait. `IdentityLift`, `ComposedLift` are
//! polymorphic over D. `ShapeLift` is the single universal
//! struct that absorbs every library shape-lift; concrete
//! shape-lifts are constructor functions on each capable domain
//! (`Shared::wrap_init_lift`, `Local::explainer_lift`, etc.).
//!
//! Capability markers (`PureLift`, `ShareableLift`) and the
//! `ShapeCapable<N>` trait live in `capability`.

pub mod core;
pub mod identity;
pub mod composed;
pub mod capability;
pub mod shape;
pub mod seed_lift;

pub use core::Lift;
pub use identity::IdentityLift;
pub use composed::ComposedLift;
pub use capability::{ShapeCapable, PureLift, ShareableLift};
pub use shape::ShapeLift;
pub use seed_lift::SeedLift;
