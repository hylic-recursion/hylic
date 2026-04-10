//! Graph types and composition.
//!
//! Edgy<N, E> is the general edge function. Treeish<N> = Edgy<N, N>
//! is the tree traversal type implementing TreeOps. Both are Arc-based,
//! Clone, Send+Sync — enabling graph composition (Graph, SeedGraph).
//!
//! Domain-independent: any executor accepts &impl TreeOps<N>.
//! The fold's domain is a separate choice.

pub mod edgy;
pub mod compose;
pub(crate) mod combinators;
pub mod visit;

pub use edgy::{
    Edgy, Treeish,
    edgy, edgy_visit,
    treeish, treeish_visit, treeish_from,
};
pub use compose::{Graph, SeedGraph, GraphWithFold};
pub use visit::{Visit, visit_slice};
