//! LiftOps — the lift operations abstraction.
//!
//! Domain-generic: works with Shared, Local, or Owned (subject to
//! Clone availability for run_lifted).

use crate::domain::Domain;

/// The four lift operations, independent of storage.
pub trait LiftOps<D, N: 'static, H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static>
where
    D: Domain<N> + Domain<N2>,
{
    fn lift_treeish(&self, t: <D as Domain<N>>::Treeish) -> <D as Domain<N2>>::Treeish;
    fn lift_fold(&self, f: <D as Domain<N>>::Fold<H, R>) -> <D as Domain<N2>>::Fold<H2, R2>;
    fn lift_root(&self, root: &N) -> N2;
    fn unwrap(&self, result: R2) -> R;
}
