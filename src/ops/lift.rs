//! LiftOps — the lift operations abstraction.
//!
//! Lifts operate on Shared-domain types: Arc-based Fold and Treeish.
//! They transform a computation (fold + graph) to a different type
//! domain while preserving the execution semantics.
//!
//! The lifted heap type is a GAT (`LiftedH<H>`) — each lift determines
//! how H maps to H2. This keeps H out of the trait's type parameters,
//! making unwrap and other non-heap methods freely inferrable.

use crate::domain::shared;
use crate::graph;

/// The four lift operations. Shared-domain: uses Arc-based Fold and Treeish.
///
/// `LiftedH<H>` maps the original heap type to the lifted heap type.
/// Each lift implementation defines this mapping (e.g., SeedHeap<H, R>,
/// ExplainerHeap<N, H, R>, LazyHeap<H, R>).
pub trait LiftOps<N: 'static, R: 'static, N2: 'static, R2: 'static> {
    type LiftedH<H: 'static>: 'static;

    fn lift_treeish(&self, t: graph::Treeish<N>) -> graph::Treeish<N2>;
    fn lift_fold<H: 'static>(&self, f: shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, Self::LiftedH<H>, R2>;
    fn lift_root(&self, root: &N) -> N2;
    fn unwrap(&self, result: R2) -> R;
}
