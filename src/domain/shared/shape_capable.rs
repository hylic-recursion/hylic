//! `impl ShapeCapable<N> for Shared` — Arc-backed xform storage,
//! Send+Sync on all closures. Enables `ShapeLift<Shared, …>`.

use std::sync::Arc;
use crate::domain::{Domain, Shared};
use crate::domain::shared::fold::Fold;
use crate::graph::Edgy;
use crate::ops::lift::capability::ShapeCapable;

impl<N: 'static> ShapeCapable<N> for Shared {
    type GrowXform<N2: 'static> =
        Arc<dyn Fn(&N) -> N2 + Send + Sync>;

    type TreeishXform<N2: 'static> =
        Arc<dyn Fn(&Edgy<N, N>) -> Edgy<N2, N2> + Send + Sync>;

    type FoldXform<H, R, N2, H2, R2> =
        Arc<dyn Fn(Fold<N, H, R>) -> Fold<N2, H2, R2> + Send + Sync>
        where H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static;

    fn apply_grow_xform<Seed: 'static, N2: 'static>(
        t: &Self::GrowXform<N2>,
        g: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
    ) -> Arc<dyn Fn(&Seed) -> N2 + Send + Sync>
    where Self: Domain<N2>,
    {
        let t = t.clone();
        Arc::new(move |s: &Seed| t(&g(s)))
    }

    fn apply_treeish_xform<N2: 'static>(
        t: &Self::TreeishXform<N2>,
        g: Edgy<N, N>,
    ) -> Edgy<N2, N2>
    where Self: Domain<N2>,
    {
        t(&g)
    }

    fn apply_fold_xform<H, R, N2, H2, R2>(
        t: &Self::FoldXform<H, R, N2, H2, R2>,
        f: Fold<N, H, R>,
    ) -> Fold<N2, H2, R2>
    where Self: Domain<N2>,
          H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static,
    {
        t(f)
    }

    fn identity_grow_xform() -> Self::GrowXform<N>
    where N: Clone,
    {
        Arc::new(|n: &N| n.clone())
    }

    fn identity_treeish_xform() -> Self::TreeishXform<N>
    where N: Clone,
    {
        Arc::new(|g: &Edgy<N, N>| g.clone())
    }

    fn identity_fold_xform<H: 'static, R: 'static>() -> Self::FoldXform<H, R, N, H, R> {
        Arc::new(|f: Fold<N, H, R>| f)
    }
}
