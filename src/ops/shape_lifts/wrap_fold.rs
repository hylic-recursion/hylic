//! WrapInitLift — wraps the fold's init closure.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::graph::{Edgy, Treeish};
use crate::domain::shared::fold::Fold;
use crate::ops::lift::Lift;

pub struct WrapInitLift<N, H, W> {
    wrapper: Arc<W>,
    _m: PhantomData<fn() -> (N, H)>,
}

impl<N, H, W> Clone for WrapInitLift<N, H, W> {
    fn clone(&self) -> Self { WrapInitLift { wrapper: self.wrapper.clone(), _m: PhantomData } }
}

pub fn wrap_init_lift<N, H, W>(wrapper: W) -> WrapInitLift<N, H, W>
where W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
{
    WrapInitLift { wrapper: Arc::new(wrapper), _m: PhantomData }
}

impl<N, Seed, H, R, W> Lift<N, Seed, H, R> for WrapInitLift<N, H, W>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
      W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
{
    type N2 = N;
    type Seed2 = Seed;
    type MapH = H;
    type MapR = R;

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
            Fold<N, H, R>,
        ) -> T,
    ) -> T {
        let w = self.wrapper.clone();
        let wrapped = fold.wrap_init(move |n: &N, orig: &dyn Fn(&N) -> H| w(n, orig));
        cont(grow, seeds, treeish, wrapped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}
