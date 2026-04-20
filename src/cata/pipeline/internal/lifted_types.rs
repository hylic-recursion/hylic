// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! LiftedNode — type-level structure of SeedLift's output treeish.
//!
//! Two variants:
//!   - `Entry`: root branching point. Visiting Entry fans out to the
//!     user-supplied entry seeds, each grown to a Node via SeedLift's
//!     grow closure.
//!   - `Node(N)`: a resolved node. Visiting Node delegates to the
//!     base treeish; init/accumulate/finalize flow through the base
//!     fold.
//!
//! There is no intermediate "Seed" variant and no "Relay" heap. An
//! earlier design modelled a deferred-grow state (Seed child that
//! later resolves to a Node); the current design grows inline at
//! Entry-visit time, so such states are never observed.

#[derive(Clone)]
pub enum LiftedNode<N> {
    Entry,
    Node(N),
}
