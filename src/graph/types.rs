use std::sync::Arc;
use either::Either;
use crate::prelude::visit::Visit;

#[derive(Clone)]
pub struct Edgy<NodeT, EdgeT> {
    impl_visit: Arc<dyn Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + Send + Sync>,
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

    pub fn map<F, NewEdgeT>(&self, transform: F) -> Edgy<NodeT, NewEdgeT>
    where F: Fn(&EdgeT) -> NewEdgeT + Send + Sync + 'static,
    {
        let inner = self.impl_visit.clone();
        edgy_visit(move |n: &NodeT, cb: &mut dyn FnMut(&NewEdgeT)| {
            inner(n, &mut |e: &EdgeT| {
                let mapped = transform(e);
                cb(&mapped);
            });
        })
    }

    pub fn contramap<F, NewNodeT>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> NodeT + Send + Sync + 'static,
    {
        let inner = self.impl_visit.clone();
        edgy_visit(move |n: &NewNodeT, cb: &mut dyn FnMut(&EdgeT)| {
            inner(&transform(n), cb);
        })
    }

    pub fn contramap_or<F, NewNodeT>(&self, transform: F) -> Edgy<NewNodeT, EdgeT>
    where F: Fn(&NewNodeT) -> Either<NodeT, Vec<EdgeT>> + Send + Sync + 'static,
    {
        let inner = self.impl_visit.clone();
        edgy_visit(move |n: &NewNodeT, cb: &mut dyn FnMut(&EdgeT)| {
            match transform(n) {
                Either::Left(node) => inner(&node, cb),
                Either::Right(edges) => { for e in &edges { cb(e); } }
            }
        })
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

pub type Treeish<Node> = Edgy<Node, Node>;

// Direct callback constructor — zero allocation traversal
pub fn edgy_visit<NodeT, EdgeT>(
    func: impl Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + Send + Sync + 'static,
) -> Edgy<NodeT, EdgeT> {
    Edgy { impl_visit: Arc::from(
        Box::new(func) as Box<dyn Fn(&NodeT, &mut dyn FnMut(&EdgeT)) + Send + Sync>
    )}
}

pub fn treeish_visit<NodeT>(
    func: impl Fn(&NodeT, &mut dyn FnMut(&NodeT)) + Send + Sync + 'static,
) -> Treeish<NodeT> {
    edgy_visit(func)
}

// Compat: Vec-returning constructors, wraps into callback internally
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
