//! Fold infrastructure — domain-independent combinator logic.
//!
//! The concrete Fold types live in their domain modules:
//! - `domain::shared::fold::Fold` (Arc-based)
//! - `domain::local::Fold` (Rc-based)
//! - `domain::owned::Fold` (Box-based)

pub(crate) mod combinators;
