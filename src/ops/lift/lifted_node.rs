//! LiftedNode — type-level structure of SeedLift's output treeish.
//!
//! Represents one of two cases inside the seed-closed chain:
//!   - the synthetic Entry root (children = user's entry seeds, each
//!     grown to a Node via SeedLift's grow closure), or
//!   - a resolved Node(N) that delegates to the base treeish/fold.
//!
//! The variants are library-internal (`pub(crate)`): construction and
//! pattern-matching happen inside SeedLift's `apply` and the
//! `LiftedSeedPipeline` sugars that dispatch on them. User code
//! inspects via the accessor methods below.
//!
//! Sealing rationale: Without sealing, every result type that carries
//! `N` at the chain's tip (e.g. `ExplainerResult<LiftedNode<N>, H, R>`)
//! would expose the `Entry`/`Node(N)` variants to pattern-matching.
//! The sealed form keeps the type's identity (so users can name it in
//! result annotations) but forces inspection through three helpers:
//! `is_entry`, `as_node`, `map_node`. The Entry/Node split is an
//! internal mechanism; callers think in terms of "is this the
//! top-level fan-out row, or a real N".
//!
//! Lives in core with `SeedLift` — `LiftedNode<N>` is SeedLift's
//! `N2` associated type and must be visible wherever `SeedLift`
//! is constructed or applied.

// ANCHOR: lifted_node_enum
/// Opaque row type in a seed-closed chain's treeish. Values are
/// either the synthetic Entry row (seed fan-out) or a resolved
/// `Node(N)`. User code inspects via [`is_entry`](Self::is_entry),
/// [`as_node`](Self::as_node), [`into_node`](Self::into_node), and
/// [`map_node`](Self::map_node); the variants are sealed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiftedNode<N> {
    // Exposed `pub` (not `pub(crate)`) so the doc-hidden
    // `lifted_node_internal` module can re-export it for
    // `hylic-pipeline`'s dispatch. User code should treat this field
    // as opaque and use `is_entry` / `as_node` / `map_node`.
    #[doc(hidden)]
    pub inner: LiftedNodeInner<N>,
}

/// Library-internal variant carrier for `LiftedNode<N>`. Exposed
/// `pub` only to make crate-external re-export through the
/// `lifted_node_internal` doc-hidden module possible. User code
/// should never name this directly.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiftedNodeInner<N> {
    Entry,
    Node(N),
}
// ANCHOR_END: lifted_node_enum

impl<N> LiftedNode<N> {
    /// Construct the synthetic Entry row. Exposed `pub` but
    /// doc-hidden: user code never builds a `LiftedNode<N>` directly.
    #[doc(hidden)]
    #[inline]
    pub fn entry() -> Self {
        Self { inner: LiftedNodeInner::Entry }
    }

    /// Construct a resolved Node. Exposed `pub` but doc-hidden:
    /// reserved for `hylic-pipeline`'s internal dispatch.
    #[doc(hidden)]
    #[inline]
    pub fn node(n: N) -> Self {
        Self { inner: LiftedNodeInner::Node(n) }
    }

    /// True if this row is the synthetic Entry fan-out node (the root
    /// of a seed-closed chain's treeish). User code inspects here
    /// when a result type surfaces `LiftedNode<N>`.
    #[inline]
    pub fn is_entry(&self) -> bool {
        matches!(self.inner, LiftedNodeInner::Entry)
    }

    /// Return `Some(&n)` for a resolved node, `None` for the Entry
    /// row. Sealed counterpart to pattern-matching.
    #[inline]
    pub fn as_node(&self) -> Option<&N> {
        match &self.inner {
            LiftedNodeInner::Node(n) => Some(n),
            LiftedNodeInner::Entry   => None,
        }
    }

    /// Consume self and return `Some(n)` for a resolved node, `None`
    /// for Entry.
    #[inline]
    pub fn into_node(self) -> Option<N> {
        match self.inner {
            LiftedNodeInner::Node(n) => Some(n),
            LiftedNodeInner::Entry   => None,
        }
    }

    /// Map the inner `N → M` for a resolved node; leave Entry
    /// unchanged. Keeps the seal tight: user code never sees the
    /// variants directly, only this functorial operation.
    #[inline]
    pub fn map_node<M, F>(&self, f: F) -> LiftedNode<M>
    where F: FnOnce(&N) -> M,
    {
        match &self.inner {
            LiftedNodeInner::Node(n) => LiftedNode::node(f(n)),
            LiftedNodeInner::Entry   => LiftedNode::entry(),
        }
    }
}
