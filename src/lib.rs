//! hylic — decomposed recursive tree computation.
//!
//! Separates what to compute ([`ops::FoldOps`]) from the tree structure
//! ([`ops::TreeOps`]) and the executor ([`cata::exec::Executor`]).
//! Each piece is independently definable, transformable, and composable.
//!
//! Three boxing domains ([`domain`]) control how closures are stored:
//! [`domain::Shared`] (Arc), [`domain::Local`] (Rc), [`domain::Owned`] (Box).

pub mod ops;
pub mod domain;

pub(crate) mod graph;
pub(crate) mod fold;
pub mod cata;
pub(crate) mod pipeline;

pub mod prelude;
