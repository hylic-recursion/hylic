//! Edgy and Treeish — Arc-based graph types.
//!
//! One primitive: `map_endpoints(rewrite_visit)`. Every sugar
//! (`map`, `contramap`, `contramap_or`, `filter`) is a one-line
//! wrapper that constructs the right visit-rewrite closure.

use std::sync::Arc;
use either::Either;
use crate::ops::{TreeOps, GraphTransformsByRef};
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

    // ── map_endpoints — sole slot-level primitive ──

    /// Rewrite the stored visit callback: the sole primitive for
    /// producing a new `Edgy<N2, E2>` from this one. Every sugar
    /// below is a one-line wrapper over `map_endpoints`.
    pub fn map_endpoints<N2, E2, MV>(
        &self,
        rewrite_visit: MV,
    ) -> Edgy<N2, E2>
    where
        N2: 'static, E2: 'static,
        MV: FnOnce(Arc<dyn Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + Send + Sync>)
                   -> Arc<dyn Fn(&N2, &mut dyn FnMut(&E2)) + Send + Sync>,
    {
        Edgy { impl_visit: rewrite_visit(self.impl_visit.clone()) }
    }

    // ── Sugars — one-liners over map_endpoints ──

    // ANCHOR: edgy_map
    pub fn map<F, NewEdgeT: 'static>(&self, transform: F) -> Edgy<NodeT, NewEdgeT>
    where F: Fn(&EdgeT) -> NewEdgeT + Send + Sync + 'static,
    {
        self.map_endpoints(move |inner| {
            Arc::new(move |n: &NodeT, cb: &mut dyn FnMut(&NewEdgeT)| {
                inner(n, &mut |e: &EdgeT| cb(&transform(e)))
            })
        })
    }
    // ANCHOR_END: edgy_map

    // ANCHOR: edgy_contramap
    pub fn contramap<F, NewNodeT: 'static>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> NodeT + Send + Sync + 'static,
    {
        self.map_endpoints(move |inner| {
            Arc::new(move |n: &NewNodeT, cb: &mut dyn FnMut(&EdgeT)| {
                inner(&transform(n), cb)
            })
        })
    }
    // ANCHOR_END: edgy_contramap

    // ANCHOR: edgy_contramap_or
    pub fn contramap_or<F, NewNodeT: 'static>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> Either<NodeT, Vec<EdgeT>> + Send + Sync + 'static,
    {
        self.map_endpoints(move |inner| {
            Arc::new(move |n: &NewNodeT, cb: &mut dyn FnMut(&EdgeT)| {
                match transform(n) {
                    Either::Left(node) => inner(&node, cb),
                    Either::Right(edges) => { for e in &edges { cb(e); } }
                }
            })
        })
    }
    // ANCHOR_END: edgy_contramap_or

    pub fn filter(&self, pred: impl Fn(&EdgeT) -> bool + Send + Sync + 'static) -> Self {
        self.map_endpoints(move |inner| {
            Arc::new(move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| {
                inner(n, &mut |e: &EdgeT| if pred(e) { cb(e); })
            })
        })
    }
}

impl<NodeT, EdgeT> GraphTransformsByRef<NodeT, EdgeT> for Edgy<NodeT, EdgeT>
where NodeT: 'static, EdgeT: 'static,
{
    type Visit = Arc<dyn Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + Send + Sync>;
    type Out<N2, E2> = Edgy<N2, E2> where N2: 'static, E2: 'static;
    type OutVisit<N2, E2> =
        Arc<dyn Fn(&N2, &mut dyn FnMut(&E2)) + Send + Sync>
        where N2: 'static, E2: 'static;

    fn map_endpoints<N2, E2, MV>(&self, rewrite_visit: MV) -> Edgy<N2, E2>
    where
        N2: 'static, E2: 'static,
        MV: FnOnce(Self::Visit) -> Self::OutVisit<N2, E2>,
    {
        Edgy { impl_visit: rewrite_visit(self.impl_visit.clone()) }
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
