//! TreeOps — the tree traversal abstraction.
//!
//! Any type implementing visit (callback-based child traversal)
//! can serve as a graph for an executor. Domain-specific types
//! (shared::Treeish, local::Treeish, owned::Treeish) implement
//! this, as can user-defined structs (e.g. adjacency lists).

// ANCHOR: treeops_trait
/// Tree traversal operations, independent of storage.
pub trait TreeOps<N> {
    /// Visit children of `node` via callback. Zero allocation.
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N));

    /// Visit with a monomorphized callback — avoids the `dyn FnMut`
    /// vtable dispatch that `visit` requires.
    fn visit_inline<F: FnMut(&N)>(&self, node: &N, cb: &mut F)
    where Self: Sized
    {
        self.visit(node, cb)
    }

    /// Collect children to Vec. Default: collect via visit.
    fn apply(&self, node: &N) -> Vec<N> where N: Clone {
        let mut v = Vec::new();
        self.visit(node, &mut |child| v.push(child.clone()));
        v
    }

    /// Pull-based: get first child + a cursor for remaining siblings.
    /// The cursor is OWNED — sendable to another thread.
    /// Default: collect via visit, split into first + rest.
    fn first_child(&self, node: &N) -> Option<(N, ChildCursor<N>)>
    where N: Clone, Self: Sized
    {
        let mut children = Vec::new();
        self.visit(node, &mut |child| children.push(child.clone()));
        if children.is_empty() { return None; }
        let first = children.remove(0);
        Some((first, ChildCursor(children)))
    }
}
// ANCHOR_END: treeops_trait

/// Owned cursor over remaining siblings. Send + 'static.
/// Pull one child at a time via `next()`.
pub struct ChildCursor<N>(Vec<N>);

impl<N> ChildCursor<N> {
    /// Pull the next child. Returns the child + a cursor for the rest.
    /// Returns None if no more children.
    pub fn next(mut self) -> Option<(N, ChildCursor<N>)> {
        if self.0.is_empty() {
            None
        } else {
            let first = self.0.remove(0);
            Some((first, ChildCursor(self.0)))
        }
    }

    /// True if no more children.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
