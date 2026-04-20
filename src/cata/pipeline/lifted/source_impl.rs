//! impl PipelineSource for LiftedPipeline<Base, L> — delegates to
//! the base's PipelineSource impl, then runs the lift chain's
//! apply on the yielded triple. Domain flows from Base.

use crate::domain::Domain;
use crate::ops::Lift;
use super::LiftedPipeline;
use super::super::source::PipelineSource;

impl<Base, L> PipelineSource for LiftedPipeline<Base, L>
where Base: PipelineSource,
      Base::Domain: Domain<L::N2>,
      L: Lift<Base::Domain, Base::N, Base::H, Base::R>,
      L::N2:   Clone + 'static,
      L::MapH: Clone + 'static,
      L::MapR: Clone + 'static,
{
    type Domain = Base::Domain;
    type Seed = Base::Seed;
    type N    = L::N2;
    type H    = L::MapH;
    type R    = L::MapR;

    fn with_constructed<T>(
        &self,
        cont: impl FnOnce(
            <Self::Domain as Domain<Self::N>>::Grow<Self::Seed, Self::N>,
            <Self::Domain as Domain<Self::N>>::Graph<Self::N>,
            <Self::Domain as Domain<Self::N>>::Fold<Self::H, Self::R>,
        ) -> T,
    ) -> T {
        self.base.with_constructed(|grow, treeish, fold| {
            self.pre_lift.apply::<Base::Seed, _>(grow, treeish, fold, cont)
        })
    }
}
