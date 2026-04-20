//! OwnedPipeline — one-shot pipeline over the Owned domain.
//!
//! Not Clone; `run_from_node` consumes self. The fold and edgy
//! are `Box`-stored (zero refcount). Does NOT compose shape-lifts
//! (Owned is not `ShapeCapable`); users supply a complete fold at
//! construction and run once.

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
    type Seed = ();
    type N    = N;
    type H    = H;
    type R    = R;

    fn with_constructed_once<T>(
        self,
        cont: impl FnOnce(
            <Owned as Domain<N>>::Grow<(), N>,
            <Owned as Domain<N>>::Graph<N>,
            <Owned as Domain<N>>::Fold<H, R>,
        ) -> T,
    ) -> T {
        let grow = <Owned as Domain<N>>::make_grow(|_: &()| {
            unreachable!("OwnedPipeline has no Seed→N step; \
                          use run_from_node_once")
        });
        cont(grow, self.treeish, self.fold)
    }
}
