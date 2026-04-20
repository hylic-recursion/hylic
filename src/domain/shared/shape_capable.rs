//! `impl ShapeCapable<N> for Shared` — Arc-backed xform storage,
//! Send+Sync on all closures. Enables `ShapeLift<Shared, …>`.
//!
//! Body uses concrete-type helper fns for each applicator because
//! Rust's GAT normalisation doesn't reduce `<Shared as Domain<N2>>::X`
//! to the concrete Arc/Edgy/Fold type inside a generic impl method
//! body. The helpers pin Self = Shared, so the GAT reduces in their
//! scope.

use std::sync::Arc;
use crate::domain::{Domain, Shared};
use crate::domain::shared::fold::Fold;
use crate::graph::Edgy;
use crate::ops::lift::capability::ShapeCapable;

// ── Helpers: pin Self = Shared so the GAT reduces ────────

fn shared_grow<Seed: 'static, NOut: 'static>(
    closure: Arc<dyn Fn(&Seed) -> NOut + Send + Sync>,
) -> <Shared as Domain<NOut>>::Grow<Seed, NOut> {
    closure
}

fn shared_graph<NodeT: 'static>(
    edgy: Edgy<NodeT, NodeT>,
) -> <Shared as Domain<NodeT>>::Graph<NodeT> {
    edgy
}

fn shared_fold<NodeT: 'static, H: 'static, R: 'static>(
    fold: Fold<NodeT, H, R>,
) -> <Shared as Domain<NodeT>>::Fold<H, R> {
    fold
}

// ── Impl ─────────────────────────────────────────────────

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
        g: <Self as Domain<N>>::Grow<Seed, N>,
    ) -> <Self as Domain<N2>>::Grow<Seed, N2>
    where Self: Domain<N2>,
    {
        let t = t.clone();
        let inner: Arc<dyn Fn(&Seed) -> N2 + Send + Sync> =
            Arc::new(move |s: &Seed| t(&g(s)));
        shared_grow::<Seed, N2>(inner)
    }

    fn apply_treeish_xform<N2: 'static>(
        t: &Self::TreeishXform<N2>,
        g: <Self as Domain<N>>::Graph<N>,
    ) -> <Self as Domain<N2>>::Graph<N2>
    where Self: Domain<N2>,
    {
        let new_edgy: Edgy<N2, N2> = t(&g);
        shared_graph::<N2>(new_edgy)
    }

    fn apply_fold_xform<H, R, N2, H2, R2>(
        t: &Self::FoldXform<H, R, N2, H2, R2>,
        f: <Self as Domain<N>>::Fold<H, R>,
    ) -> <Self as Domain<N2>>::Fold<H2, R2>
    where Self: Domain<N2>,
          H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static,
    {
        let new_fold: Fold<N2, H2, R2> = t(f);
        shared_fold::<N2, H2, R2>(new_fold)
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
