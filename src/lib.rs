//! hylic — decomposed recursive tree computation.
//!
//! Separates what to compute ([`ops::FoldOps`]) from the tree structure
//! ([`ops::TreeOps`]) and the executor ([`exec::Executor`]).
//! Each piece is independently definable, transformable, and composable.
//!
//! Three boxing domains ([`domain`]) control how closures are stored:
//! [`domain::Shared`] (Arc), [`domain::Local`] (Rc), [`domain::Owned`] (Box).

#![warn(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

/// Core operation traits: `FoldOps`, `TreeOps`, and the `Lift`
/// family.
pub mod ops;
/// Domain markers and their storage-specific types (Shared, Local,
/// Owned).
pub mod domain;
/// Graph / Treeish types and callback-based visit plumbing.
pub mod graph;
/// Executors and lifecycle types (`Executor`, `ExecutorSpec`,
/// `Exec<D, S>`, plus variants `Fused` and `Funnel`).
pub mod exec;

/// Common re-exports intended for `use hylic::prelude::*;`.
pub mod prelude;

#[cfg(test)]
mod tests;
