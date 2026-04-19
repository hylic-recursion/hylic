//! IdentityLift: passes everything through.

use crate::graph::Treeish;
use crate::domain::shared::fold::Fold;
use super::lift::Lift;

#[derive(Clone, Copy)]
pub struct IdentityLift;

impl<N: Clone + 'static> Lift<N, N> for IdentityLift {
    type MapH<H: Clone + 'static, R: Clone + 'static> = H;
    type MapR<H: Clone + 'static, R: Clone + 'static> = R;

    fn lift_treeish(&self, t: Treeish<N>) -> Treeish<N> { t }
    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(&self, f: Fold<N, H, R>) -> Fold<N, H, R> { f }
    fn lift_root(&self, root: &N) -> N { root.clone() }
}
