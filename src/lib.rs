//! hylic — decomposed recursive tree computation.
//!
//! Separates what to compute ([`fold::Fold`]) from the tree structure
//! ([`graph::Treeish`]) and the executor ([`cata::Exec`]).
//! Each piece is independently definable, transformable, and composable.

pub mod uio;

pub mod graph;
pub mod fold;
pub mod cata;
pub mod pipeline;

pub mod prelude;
