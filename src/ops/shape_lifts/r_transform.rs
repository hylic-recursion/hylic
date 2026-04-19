//! ZipmapLift — augment R with a derived value. MapR = (R, Extra).

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use crate::ops::lift::Lift;

pub struct ZipmapLift<R, Extra, M> {
    mapper: Arc<M>,
    _m: PhantomData<fn() -> (R, Extra)>,
}

impl<R, Extra, M> Clone for ZipmapLift<R, Extra, M> {
    fn clone(&self) -> Self { ZipmapLift { mapper: self.mapper.clone(), _m: PhantomData } }
}

pub fn zipmap_lift<R, Extra, M>(mapper: M) -> ZipmapLift<R, Extra, M>
where M: Fn(&R) -> Extra + Send + Sync + 'static,
{
    ZipmapLift { mapper: Arc::new(mapper), _m: PhantomData }
}

impl<N, Seed, H, R, Extra, M> Lift<N, Seed, H, R> for ZipmapLift<R, Extra, M>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      Extra: Clone + 'static,
      M: Fn(&R) -> Extra + Send + Sync + 'static,
{
    type N2 = N;
    type Seed2 = Seed;
    type MapH = H;
    type MapR = (R, Extra);

    fn apply<T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds:   Edgy<N, Seed>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, H, (R, Extra)>,
        ) -> T,
    ) -> T {
        let m = self.mapper.clone();
        let zipped = fold.zipmap(move |r: &R| m(r));
        cont(grow, seeds, treeish, zipped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}
