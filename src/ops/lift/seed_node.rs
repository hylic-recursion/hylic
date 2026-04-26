//! SeedNode — type-level structure of SeedLift's output treeish.
//!
//! Represents one of two cases inside the seed-closed chain:
//!   - the synthetic `EntryRoot` (children = user's entry seeds, each
//!     grown to a Node via SeedLift's grow closure), or
//!   - a resolved `Node(N)` that delegates to the base treeish/fold.
//!
//! The variants are library-internal (`pub(crate)`-shaped via doc-hidden):
//! construction and pattern-matching happen inside SeedLift's `apply`
//! and the `Stage2Pipeline` sugars that dispatch on them. User code
//! inspects via the accessor methods below.
//!
//! Sealing rationale: Without sealing, every result type that carries
//! `N` at the chain's tip (e.g. `ExplainerResult<SeedNode<N>, H, R>`)
//! would expose the `EntryRoot`/`Node(N)` variants to pattern-matching.
//! The sealed form keeps the type's identity (so users can name it in
//! result annotations) but forces inspection through the accessors:
//! `is_entry_root`, `as_node`, `into_node`, `map_node`. The
//! EntryRoot/Node split is an internal mechanism; callers think in
//! terms of "is this the top-level fan-out row, or a real N".
//!
//! Lives in core with `SeedLift` — `SeedNode<N>` is SeedLift's
//! `N2` associated type and must be visible wherever `SeedLift`
//! is constructed or applied.

use std::fmt::{self, Debug};

// ANCHOR: seed_node_enum
/// Opaque row type in a seed-closed chain's treeish. Values are
/// either the synthetic `EntryRoot` row (seed fan-out) or a resolved
/// `Node(N)`. User code inspects via [`is_entry_root`](Self::is_entry_root),
/// [`as_node`](Self::as_node), [`into_node`](Self::into_node), and
/// [`map_node`](Self::map_node); the variants are sealed.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SeedNode<N> {
    // Exposed `pub` (not `pub(crate)`) so the doc-hidden
    // `seed_node_internal` module can re-export it for
    // `hylic-pipeline`'s dispatch. User code should treat this field
    // as opaque and use `is_entry_root` / `as_node` / `map_node`.
    #[doc(hidden)]
    pub inner: SeedNodeInner<N>,
}

/// Library-internal variant carrier for `SeedNode<N>`. Exposed
/// `pub` only to make crate-external re-export through the
/// `seed_node_internal` doc-hidden module possible. User code
/// should never name this directly.
#[doc(hidden)]
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum SeedNodeInner<N> {
    EntryRoot,
    Node(N),
}
// ANCHOR_END: seed_node_enum

impl<N: Debug> Debug for SeedNode<N> {
    /// Custom Debug that renders `<entry-root>` for the synthetic
    /// fan-out row and delegates to `N`'s Debug for resolved nodes.
    /// Keeps explainer trace output readable.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            SeedNodeInner::EntryRoot => f.write_str("<entry-root>"),
            SeedNodeInner::Node(n)   => Debug::fmt(n, f),
        }
    }
}

impl<N: Debug> Debug for SeedNodeInner<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeedNodeInner::EntryRoot => f.write_str("EntryRoot"),
            SeedNodeInner::Node(n)   => f.debug_tuple("Node").field(n).finish(),
        }
    }
}

impl<N> SeedNode<N> {
    /// Construct the synthetic EntryRoot row. Exposed `pub` but
    /// doc-hidden: user code never builds a `SeedNode<N>` directly.
    #[doc(hidden)]
    #[inline]
    pub fn entry_root() -> Self {
        Self { inner: SeedNodeInner::EntryRoot }
    }

    /// Construct a resolved Node. Exposed `pub` but doc-hidden:
    /// reserved for `hylic-pipeline`'s internal dispatch.
    #[doc(hidden)]
    #[inline]
    pub fn node(n: N) -> Self {
        Self { inner: SeedNodeInner::Node(n) }
    }

    /// Consume self and return the inner variant carrier. Doc-hidden;
    /// reserved for `Wrap::map`-style operations on the seed wrap.
    #[doc(hidden)]
    #[inline]
    pub fn into_inner(self) -> SeedNodeInner<N> {
        self.inner
    }

    /// True if this row is the synthetic EntryRoot fan-out node (the
    /// root of a seed-closed chain's treeish). User code inspects here
    /// when a result type surfaces `SeedNode<N>`.
    #[inline]
    pub fn is_entry_root(&self) -> bool {
        matches!(self.inner, SeedNodeInner::EntryRoot)
    }

    /// Return `Some(&n)` for a resolved node, `None` for the EntryRoot
    /// row. Sealed counterpart to pattern-matching.
    #[inline]
    pub fn as_node(&self) -> Option<&N> {
        match &self.inner {
            SeedNodeInner::Node(n)   => Some(n),
            SeedNodeInner::EntryRoot => None,
        }
    }

    /// Consume self and return `Some(n)` for a resolved node, `None`
    /// for EntryRoot.
    #[inline]
    pub fn into_node(self) -> Option<N> {
        match self.inner {
            SeedNodeInner::Node(n)   => Some(n),
            SeedNodeInner::EntryRoot => None,
        }
    }

    /// Map the inner `N → M` for a resolved node; leave EntryRoot
    /// unchanged. Keeps the seal tight: user code never sees the
    /// variants directly, only this functorial operation.
    #[inline]
    pub fn map_node<M, F>(&self, f: F) -> SeedNode<M>
    where F: FnOnce(&N) -> M,
    {
        match &self.inner {
            SeedNodeInner::Node(n)   => SeedNode::node(f(n)),
            SeedNodeInner::EntryRoot => SeedNode::entry_root(),
        }
    }
}
