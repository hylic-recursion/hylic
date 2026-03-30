//! Graph memoization via UIO — each child is computed at most once.
//!
//! Wraps a Treeish so that repeated traversals of the same node
//! return the cached result. Useful for DAGs where the same node
//! is reachable from multiple parents.

use crate::uio::UIO;
use crate::graph::types::Treeish;

/// Wrap a Treeish so each node's children are memoized via UIO.
/// The first traversal computes; subsequent traversals return cached results.
///
/// The returned Treeish operates on `UIO<N>` — nodes are lazy wrappers.
/// Call `.eval()` to get the underlying `&N`.
pub fn memoize_treeish<N>(graph: &Treeish<N>) -> Treeish<UIO<N>>
where N: Clone + Send + Sync + 'static
{
    graph.treemap(
        |n: &N| UIO::pure(n.clone()),
        |u: &UIO<N>| u.eval().clone(),
    )
}
