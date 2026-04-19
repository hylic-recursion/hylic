//! LiftedNode and LiftedHeap — the type-level structure of the seed
//! lift. pub(crate) visibility — these types are internal to the
//! pipeline module; users never construct them directly.

/// Node in the lifted tree.
/// - Entry: root branching point, children from a captured Edgy<(), Seed>
/// - Seed: single-child relay, grows into Node via grow
/// - Node: real node, original fold and treeish operate
#[derive(Clone)]
pub enum LiftedNode<Seed, N> {
    Entry,
    Seed(Seed),
    Node(N),
}

/// Heap in the lifted world.
/// - Active: carries the original fold's heap (for Entry and Node)
/// - Relay: pass-through slot for a single child's result (for Seed)
pub enum LiftedHeap<H, R> {
    Active(H),
    Relay(Option<R>),
}

impl<H: Clone, R: Clone> Clone for LiftedHeap<H, R> {
    fn clone(&self) -> Self {
        match self {
            LiftedHeap::Active(h) => LiftedHeap::Active(h.clone()),
            LiftedHeap::Relay(r) => LiftedHeap::Relay(r.clone()),
        }
    }
}
