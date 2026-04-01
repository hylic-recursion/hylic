//! TreeOps — the tree traversal abstraction.
//!
//! Any type implementing visit (callback-based child traversal)
//! can serve as a graph for an executor. Domain-specific types
//! (shared::Treeish, local::Treeish, owned::Treeish) implement
//! this, as can user-defined structs (e.g. adjacency lists).

/// Tree traversal operations, independent of storage.
pub trait TreeOps<N> {
    /// Visit children of `node` via callback. Zero allocation.
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N));

    /// Collect children to Vec. Default: collect via visit.
    fn apply(&self, node: &N) -> Vec<N> where N: Clone {
        let mut v = Vec::new();
        self.visit(node, &mut |child| v.push(child.clone()));
        v
    }
}
