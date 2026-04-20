// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! OwnedPipeline — one-shot, seedless, by-value pipeline over the
//! Owned domain.
//!
//! Not Clone; run_from_node_once consumes self. Not ShapeCapable —
//! doesn't compose shape-lifts. Impls `PipelineSourceOnce` (by-value
//! analogue of TreeishSource).

use crate::domain::{Domain, Owned};
use crate::domain::owned::Fold;
use crate::domain::owned::edgy::Edgy;
use super::source::PipelineSourceOnce;

pub struct OwnedPipeline<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    pub(crate) treeish: Edgy<N, N>,
    pub(crate) fold:    Fold<N, H, R>,
}

impl<N, H, R> OwnedPipeline<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    pub fn new(treeish: Edgy<N, N>, fold: Fold<N, H, R>) -> Self {
        OwnedPipeline { treeish, fold }
    }
}

impl<N, H, R> PipelineSourceOnce for OwnedPipeline<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    type Domain = Owned;
    type N    = N;
    type H    = H;
    type R    = R;

    fn with_constructed_once<T>(
        self,
        cont: impl FnOnce(
            <Owned as Domain<N>>::Graph<N>,
            <Owned as Domain<N>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        cont(self.treeish, self.fold)
    }
}
