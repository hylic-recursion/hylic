//! Graph infrastructure — domain-independent combinators and Visit iterator.
//!
//! The concrete graph types live in their domain modules:
//! - `domain::shared::graph` (Arc-based Edgy/Treeish)
//! - `domain::local::Treeish` (Rc-based)
//! - `domain::owned::Treeish` (Box-based)

pub(crate) mod combinators;
pub mod visit;
