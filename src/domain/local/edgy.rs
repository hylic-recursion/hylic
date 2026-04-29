//! Local-domain Edgy — Rc-based visit closure. Accepts non-Send
//! closures at construction and transformation sites (no Send+Sync
//! bound on user-supplied closures, unlike the Arc-based shared
//! Edgy).
//!
//! `Treeish<N>` = `Edgy<N, N>` implements `TreeOps<N>`, so executors
//! accept it (under Fused; not Funnel since Rc isn't Send+Sync).

#![allow(missing_docs)] // implementation surface; items documented at the trait/type they implement

use std::rc::Rc;
use either::Either;
use crate::ops::{TreeOps, GraphTransformsByRef};
use crate::graph::visit::Visit;

pub struct Edgy<NodeT, EdgeT> {
    impl_visit: Rc<dyn Fn(&NodeT, &mut dyn FnMut(&EdgeT))>,
}

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

    // ── Sugars — one-liners over map_endpoints ──

    pub fn map<F, NewEdgeT: 'static>(&self, transform: F) -> Edgy<NodeT, NewEdgeT>
    where F: Fn(&EdgeT) -> NewEdgeT + 'static,
    {
        <Self as GraphTransformsByRef<NodeT, EdgeT>>::map_endpoints::<NodeT, NewEdgeT, _>(
            self,
            move |inner| {
                Rc::new(move |n: &NodeT, cb: &mut dyn FnMut(&NewEdgeT)| {
                    inner(n, &mut |e: &EdgeT| cb(&transform(e)))
                })
            },
        )
    }

    pub fn contramap<F, NewNodeT: 'static>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> NodeT + 'static,
    {
        <Self as GraphTransformsByRef<NodeT, EdgeT>>::map_endpoints::<NewNodeT, EdgeT, _>(
            self,
            move |inner| {
                Rc::new(move |n: &NewNodeT, cb: &mut dyn FnMut(&EdgeT)| {
                    inner(&transform(n), cb)
                })
            },
        )
    }

    pub fn contramap_or_emit<F, NewNodeT: 'static>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> Either<NodeT, Vec<EdgeT>> + 'static,
    {
        <Self as GraphTransformsByRef<NodeT, EdgeT>>::map_endpoints::<NewNodeT, EdgeT, _>(
            self,
            move |inner| {
                Rc::new(move |n: &NewNodeT, cb: &mut dyn FnMut(&EdgeT)| {
                    match transform(n) {
                        Either::Left(node) => inner(&node, cb),
                        Either::Right(edges) => { for e in &edges { cb(e); } }
                    }
                })
            },
        )
    }

    pub fn filter(&self, pred: impl Fn(&EdgeT) -> bool + 'static) -> Self {
        <Self as GraphTransformsByRef<NodeT, EdgeT>>::map_endpoints::<NodeT, EdgeT, _>(
            self,
            move |inner| {
                Rc::new(move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| {
                    inner(n, &mut |e: &EdgeT| if pred(e) { cb(e); })
                })
            },
        )
    }
}

impl<NodeT, EdgeT> GraphTransformsByRef<NodeT, EdgeT> for Edgy<NodeT, EdgeT>
where NodeT: 'static, EdgeT: 'static,
{
    type Visit = Rc<dyn Fn(&NodeT, &mut dyn FnMut(&EdgeT))>;
    type Out<N2, E2> = Edgy<N2, E2> where N2: 'static, E2: 'static;
    type OutVisit<N2, E2> = Rc<dyn Fn(&N2, &mut dyn FnMut(&E2))> where N2: 'static, E2: 'static;

    fn map_endpoints<N2, E2, MV>(&self, rewrite_visit: MV) -> Edgy<N2, E2>
    where N2: 'static, E2: 'static,
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
}

pub type Treeish<Node> = Edgy<Node, Node>;

impl<N: 'static> TreeOps<N> for Treeish<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N)) {
        Edgy::visit(self, node, cb)
    }
    fn apply(&self, node: &N) -> Vec<N> where N: Clone {
        Edgy::apply(self, node)
    }
}

// ── Constructors ──────────────────────────────────

pub fn edgy_visit<NodeT, EdgeT>(
    func: impl Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + 'static,
) -> Edgy<NodeT, EdgeT> {
    Edgy { impl_visit: Rc::new(func) }
}

pub fn treeish_visit<NodeT>(
    func: impl Fn(&NodeT, &mut dyn FnMut(&NodeT)) + 'static,
) -> Treeish<NodeT> {
    edgy_visit(func)
}

pub fn edgy<NodeT, EdgeT>(
    func: impl Fn(&NodeT) -> Vec<EdgeT> + 'static,
) -> Edgy<NodeT, EdgeT> {
    edgy_visit(move |n: &NodeT, cb: &mut dyn FnMut(&EdgeT)| {
        for e in &func(n) { cb(e); }
    })
}

pub fn treeish<NodeT>(
    func: impl Fn(&NodeT) -> Vec<NodeT> + 'static,
) -> Treeish<NodeT> {
    edgy(func)
}
