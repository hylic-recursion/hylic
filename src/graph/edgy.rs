//! Edgy and Treeish — Arc-based graph types with full combinators.
//!
//! Domain-independent: used by all executors via TreeOps. Arc storage
//! enables Clone for graph composition (Graph, SeedGraph).

use std::sync::Arc;
use either::Either;
use crate::ops::TreeOps;
use crate::graph::combinators;
use crate::graph::visit::Visit;

// ANCHOR: edgy_struct
pub struct Edgy<NodeT, EdgeT> {
    impl_visit: Arc<dyn Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + Send + Sync>,
}
// ANCHOR_END: edgy_struct

impl<NodeT, EdgeT> Clone for Edgy<NodeT, EdgeT> {
    fn clone(&self) -> Self { Edgy { impl_visit: self.impl_visit.clone() } }
}

impl<NodeT, EdgeT> Edgy<NodeT, EdgeT>
where NodeT: 'static, EdgeT: 'static,
{
    pub fn visit(&self, node: &NodeT, cb: &mut dyn FnMut(&EdgeT)) {
        (self.impl_visit)(node, cb)
    }

    pub fn at<'a>(&'a self, node: &'a NodeT) -> Visit<EdgeT, impl FnMut(&mut dyn FnMut(&EdgeT)) + 'a> {
        let f = &self.impl_visit;
        Visit::new(move |cb: &mut dyn FnMut(&EdgeT)| f(node, cb))
    }

    pub fn apply(&self, input: &NodeT) -> Vec<EdgeT> where EdgeT: Clone {
        self.at(input).collect_vec()
    }

    pub fn map<F, NewEdgeT: 'static>(&self, transform: F) -> Edgy<NodeT, NewEdgeT>
    where F: Fn(&EdgeT) -> NewEdgeT + Send + Sync + 'static,
    {
        let inner = self.impl_visit.clone();
        edgy_visit(combinators::map_edges(
            move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| inner(n, cb), transform,
        ))
    }

    pub fn contramap<F, NewNodeT: 'static>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> NodeT + Send + Sync + 'static,
    {
        let inner = self.impl_visit.clone();
        edgy_visit(combinators::contramap_node(
            move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| inner(n, cb), transform,
        ))
    }

    pub fn contramap_or<F, NewNodeT: 'static>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> Either<NodeT, Vec<EdgeT>> + Send + Sync + 'static,
    {
        let inner = self.impl_visit.clone();
        edgy_visit(combinators::contramap_or_node(
            move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| inner(n, cb), transform,
        ))
    }

    pub fn filter(&self, pred: impl Fn(&EdgeT) -> bool + Send + Sync + 'static) -> Self {
        let inner = self.impl_visit.clone();
        edgy_visit(combinators::filter_edges(
            move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| inner(n, cb), pred,
        ))
    }
}

impl<NodeT> Edgy<NodeT, NodeT>
where NodeT: 'static,
{
    pub fn children(&self, node: &NodeT) -> Vec<NodeT> where NodeT: Clone {
        self.apply(node)
    }

    pub fn treemap<NewT>(&self,
        co_tf: impl Fn(&NodeT) -> NewT + Send + Sync + 'static,
        contra_tf: impl Fn(&NewT) -> NodeT + Send + Sync + 'static,
    ) -> Treeish<NewT> where NewT: 'static,
    {
        self.map(co_tf).contramap(contra_tf)
    }
}

// ANCHOR: treeish_alias
pub type Treeish<Node> = Edgy<Node, Node>;
// ANCHOR_END: treeish_alias

impl<N: 'static> TreeOps<N> for Treeish<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N)) {
        Edgy::visit(self, node, cb)
    }
    fn apply(&self, node: &N) -> Vec<N> where N: Clone {
        Edgy::apply(self, node)
    }
}

// ── Constructors ───────────────────────────────────

/// Direct callback constructor — zero allocation traversal.
pub fn edgy_visit<NodeT, EdgeT>(
    func: impl Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + Send + Sync + 'static,
) -> Edgy<NodeT, EdgeT> {
    Edgy { impl_visit: Arc::new(func) }
}

pub fn treeish_visit<NodeT>(
    func: impl Fn(&NodeT, &mut dyn FnMut(&NodeT)) + Send + Sync + 'static,
) -> Treeish<NodeT> {
    edgy_visit(func)
}

/// Compat: Vec-returning constructor, wraps into callback internally.
pub fn edgy<NodeT, EdgeT>(
    func: impl Fn(&NodeT) -> Vec<EdgeT> + Send + Sync + 'static,
) -> Edgy<NodeT, EdgeT> {
    edgy_visit(move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| {
        for e in &func(n) { cb(e); }
    })
}

pub fn treeish<NodeT>(
    func: impl Fn(&NodeT) -> Vec<NodeT> + Send + Sync + 'static,
) -> Treeish<NodeT> {
    edgy(func)
}

/// Construct a Treeish from a slice-returning accessor.
pub fn treeish_from<N>(
    accessor: impl Fn(&N) -> &[N] + Send + Sync + 'static,
) -> Treeish<N> where N: 'static {
    treeish_visit(move |node: &N, cb: &mut dyn FnMut(&N)| {
        for child in accessor(node) { cb(child); }
    })
}
