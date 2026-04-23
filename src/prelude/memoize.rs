//! Graph memoization — cache children so repeated visits skip the graph function.
//!
//! For DAGs where the same node is reachable from multiple parents,
//! wrapping the Treeish avoids redundant traversals. The returned
//! Treeish has the same node type — the fold doesn't change.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use crate::graph::{Treeish, treeish};

/// Memoize a Treeish using a caller-provided key function.
///
/// On first visit of a key, the original graph function runs and
/// children are cached. Subsequent visits with the same key return
/// the cached children without calling the graph.
pub fn memoize_treeish_by<N, K>(
    graph: &Treeish<N>,
    key_fn: impl Fn(&N) -> K + Send + Sync + 'static,
) -> Treeish<N>
where
    N: Clone + Send + Sync + 'static,
    K: Hash + Eq + Send + Sync + 'static,
{
    let graph = graph.clone();
    let cache: Arc<Mutex<HashMap<K, Vec<N>>>> = Arc::new(Mutex::new(HashMap::new()));
    treeish(move |node: &N| {
        let k = key_fn(node);
        // Check the cache under a short-lived lock; otherwise compute
        // children with the lock released, so an executor that
        // recurses through the memoized graph before returning cannot
        // deadlock on reentrant acquisition.
        if let Some(children) = cache.lock().unwrap().get(&k) {
            return children.clone();
        }
        let children = graph.apply(node);
        cache.lock().unwrap().insert(k, children.clone());
        children
    })
}

/// Memoize a Treeish for hashable node types.
///
/// Convenience wrapper over `memoize_treeish_by` that uses the node
/// itself as the cache key.
pub fn memoize_treeish<N>(graph: &Treeish<N>) -> Treeish<N>
where
    N: Clone + Hash + Eq + Send + Sync + 'static,
{
    memoize_treeish_by(graph, |n: &N| n.clone())
}
