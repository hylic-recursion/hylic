//! LiftOps — the lift operations abstraction.
//!
//! Lifts operate on Shared-domain types: Arc-based Fold and Treeish.
//! They transform a computation (fold + graph) to a different type
//! domain while preserving the execution semantics.

use crate::domain::shared;
use crate::graph;

/// The four lift operations. Shared-domain: uses Arc-based Fold and Treeish.
pub trait LiftOps<N: 'static, H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static> {
    fn lift_treeish(&self, t: graph::Treeish<N>) -> graph::Treeish<N2>;
    fn lift_fold(&self, f: shared::fold::Fold<N, H, R>) -> shared::fold::Fold<N2, H2, R2>;
    fn lift_root(&self, root: &N) -> N2;
    fn unwrap(&self, result: R2) -> R;
}
