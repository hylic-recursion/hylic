//! TreeOps — the tree traversal abstraction.
//! GraphTransformsByRef / GraphTransformsByValue — the graph
//! transformation abstraction, split by storage mode (mirrors Fold).

// ANCHOR: treeops_trait
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
// ANCHOR_END: treeops_trait

/// Graph transformations for domains whose storage permits cheap
/// cloning (Arc, Rc). Sole primitive is `map_endpoints`; sugars
/// (`map`, `contramap`, `contramap_or_emit`, `filter`) are inherent
/// one-line wrappers per domain.
///
/// By-reference: takes `&self`, returns a new graph over new
/// `(NodeT, EdgeT)` types.
pub trait GraphTransformsByRef<NodeT, EdgeT>: Clone + Sized
where NodeT: 'static, EdgeT: 'static,
{
    /// The domain's stored visit closure type.
    type Visit;

    /// The concrete Edgy type in this domain over new parameters.
    type Out<N2, E2>: GraphTransformsByRef<N2, E2, Visit = Self::OutVisit<N2, E2>>
    where N2: 'static, E2: 'static;

    /// Associated storage type for the output graph's visit closure.
    type OutVisit<N2, E2>: 'static where N2: 'static, E2: 'static;

    /// Rewrite the stored visit closure. Sole slot-level primitive
    /// for producing a new Edgy.
    fn map_endpoints<N2, E2, MV>(
        &self,
        rewrite_visit: MV,
    ) -> Self::Out<N2, E2>
    where
        N2: 'static, E2: 'static,
        MV: FnOnce(Self::Visit) -> Self::OutVisit<N2, E2>;
}

/// Graph transformations for domains whose storage is single-owner
/// (Box). `map_endpoints` consumes `self`.
pub trait GraphTransformsByValue<NodeT, EdgeT>: Sized
where NodeT: 'static, EdgeT: 'static,
{
    type Visit;
    type Out<N2, E2>: GraphTransformsByValue<N2, E2>
    where N2: 'static, E2: 'static;
    type OutVisit<N2, E2>: 'static where N2: 'static, E2: 'static;

    fn map_endpoints<N2, E2, MV>(
        self,
        rewrite_visit: MV,
    ) -> Self::Out<N2, E2>
    where
        N2: 'static, E2: 'static,
        MV: FnOnce(Self::Visit) -> Self::OutVisit<N2, E2>;
}
