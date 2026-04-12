//! IdentityLift: the trivial lift that passes everything through.

use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use super::lift::LiftOps;
use super::outer_lift::OuterLift;

#[derive(Clone, Copy)]
pub struct IdentityLift;

impl<N: Clone + 'static, R: Clone + 'static> LiftOps<N, R, N> for IdentityLift {
    type LiftedH<H: Clone + 'static> = H;
    type LiftedR<H: Clone + 'static> = R;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }
    fn lift_fold<H: Clone + 'static>(&self, f: Fold<N, H, R>) -> Fold<N, H, R> { f }
    fn lift_root(&self, root: &N) -> N { root.clone() }
}

impl<Inner, N, R, Nmid> OuterLift<Inner, N, R, Nmid, Nmid> for IdentityLift
where
    N: 'static,
    R: Clone + 'static,
    Nmid: Clone + 'static,
    Inner: LiftOps<N, R, Nmid>,
{
    type LiftedH<H: Clone + 'static> = Inner::LiftedH<H>;
    type LiftedR<H: Clone + 'static> = Inner::LiftedR<H>;

    fn lift_treeish(&self, t: Treeish<Nmid>) -> Treeish<Nmid> { t }

    fn lift_fold<H: Clone + 'static>(
        &self,
        f: Fold<Nmid, Inner::LiftedH<H>, Inner::LiftedR<H>>,
    ) -> Fold<Nmid, Inner::LiftedH<H>, Inner::LiftedR<H>> { f }

    fn lift_root(&self, root: &Nmid) -> Nmid { root.clone() }
}
