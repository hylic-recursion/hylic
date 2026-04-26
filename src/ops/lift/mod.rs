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
//!
//! `SeedNode<N>` is SeedLift's output type and lives here next
//! to SeedLift so both travel together with the core crate.

pub mod core;
pub mod identity;
pub mod composed;
pub mod capability;
pub mod bare;
pub mod shape;
pub mod seed_lift;
pub mod seed_node;

pub use core::Lift;
pub use identity::IdentityLift;
pub use composed::ComposedLift;
pub use capability::{ShapeCapable, PureLift, ShareableLift};
pub use bare::LiftBare;
pub use shape::ShapeLift;
pub use seed_lift::SeedLift;
pub use seed_node::SeedNode;

/// Deprecated alias — `LiftedNode<N>` is now `SeedNode<N>`. The rename
/// reflects that the type is the seed-pipeline's Entry-vs-Node carrier;
/// "Lifted" was a generic word for what is specifically the seed wrap.
#[deprecated(note = "renamed to SeedNode")]
pub type LiftedNode<N> = SeedNode<N>;

/// Library-internal access to SeedNode's variants. `hylic-pipeline`
/// imports this for the Node/EntryRoot dispatch inside the
/// `Stage2Pipeline` sugars; user code uses the `is_entry_root` /
/// `as_node` / `map_node` accessors on `SeedNode<N>` instead.
#[doc(hidden)]
pub mod seed_node_internal {
    pub use super::seed_node::SeedNodeInner;

    /// Construct an EntryRoot row. Crate-internal; hidden from docs.
    pub fn entry_root<N>() -> super::SeedNode<N> {
        super::SeedNode::entry_root()
    }
    /// Construct a Node row. Crate-internal.
    pub fn node<N>(n: N) -> super::SeedNode<N> {
        super::SeedNode::node(n)
    }
    /// Borrow the inner enum for dispatch. Crate-internal.
    pub fn inner<N>(sn: &super::SeedNode<N>) -> &SeedNodeInner<N> {
        &sn.inner
    }
}

/// Deprecated alias — `lifted_node_internal` is now `seed_node_internal`.
#[deprecated(note = "renamed to seed_node_internal")]
#[doc(hidden)]
pub mod lifted_node_internal {
    pub use super::seed_node::SeedNodeInner as LiftedNodeInner;

    pub fn entry<N>() -> super::SeedNode<N> { super::SeedNode::entry_root() }
    pub fn node<N>(n: N) -> super::SeedNode<N> { super::SeedNode::node(n) }
    pub fn inner<N>(sn: &super::SeedNode<N>) -> &super::seed_node::SeedNodeInner<N> {
        &sn.inner
    }
}
