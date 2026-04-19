//! FilterSeedsLift — filters seeds at the Edgy (pre-grow) level.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use crate::ops::lift::Lift;

pub struct FilterSeedsLift<Seed, P> {
    pred: Arc<P>,
    _s: PhantomData<fn() -> Seed>,
}

impl<Seed, P> Clone for FilterSeedsLift<Seed, P> {
    fn clone(&self) -> Self { FilterSeedsLift { pred: self.pred.clone(), _s: PhantomData } }
}

pub fn filter_seeds_lift<Seed, P>(pred: P) -> FilterSeedsLift<Seed, P>
where P: Fn(&Seed) -> bool + Send + Sync + 'static,
{
    FilterSeedsLift { pred: Arc::new(pred), _s: PhantomData }
}

impl<N, Seed, H, R, P> Lift<N, Seed, H, R> for FilterSeedsLift<Seed, P>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      P: Fn(&Seed) -> bool + Send + Sync + 'static,
{
    type N2 = N;
    type Seed2 = Seed;
    type MapH = H;
    type MapR = R;

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        _treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let pred = self.pred.clone();
        let filtered = seeds.filter(move |s: &Seed| pred(s));
        // Rebuild the treeish from filtered seeds so pre-grow filtering
        // is reflected at the treeish level.
        let new_treeish: Treeish<N> = {
            let g = grow.clone();
            filtered.clone().map(move |s: &Seed| g(s))
        };
        cont(grow, filtered, new_treeish, fold)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}
