//! `impl ShapeCapable<N> for Local` — Rc-backed xform storage,
//! no Send+Sync bound. Enables `ShapeLift<Local, …>`.
//!
//! Body uses concrete-type helper fns for each applicator because
//! Rust's GAT normalisation doesn't reduce `<Local as Domain<N2>>::X`
//! to the concrete Rc/Edgy/Fold type inside a generic impl method
//! body. The helpers pin Self = Local, so the GAT reduces in their
//! scope.

use std::rc::Rc;
use crate::domain::{Domain, Local};
use crate::domain::local::Fold;
use crate::domain::local::edgy::Edgy;
use crate::ops::lift::capability::ShapeCapable;

// ── Helpers: pin Self = Local so the GAT reduces ─────────

fn local_grow<Seed: 'static, NOut: 'static>(
    closure: Rc<dyn Fn(&Seed) -> NOut>,
) -> <Local as Domain<NOut>>::Grow<Seed, NOut> {
    closure
}

fn local_graph<NodeT: 'static>(
    edgy: Edgy<NodeT, NodeT>,
) -> <Local as Domain<NodeT>>::Graph<NodeT> {
    edgy
}

fn local_fold<NodeT: 'static, H: 'static, R: 'static>(
    fold: Fold<NodeT, H, R>,
) -> <Local as Domain<NodeT>>::Fold<H, R> {
    fold
}

// ── Impl ─────────────────────────────────────────────────

impl<N: 'static> ShapeCapable<N> for Local {
    type GrowXform<N2: 'static> =
        Rc<dyn Fn(&N) -> N2>;

    type TreeishXform<N2: 'static> =
        Rc<dyn Fn(&Edgy<N, N>) -> Edgy<N2, N2>>;

    type FoldXform<H, R, N2, H2, R2> =
        Rc<dyn Fn(Fold<N, H, R>) -> Fold<N2, H2, R2>>
        where H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static;

    fn apply_grow_xform<Seed: 'static, N2: 'static>(
        t: &Self::GrowXform<N2>,
        g: <Self as Domain<N>>::Grow<Seed, N>,
    ) -> <Self as Domain<N2>>::Grow<Seed, N2>
    where Self: Domain<N2>,
    {
        let t = t.clone();
        let inner: Rc<dyn Fn(&Seed) -> N2> = Rc::new(move |s: &Seed| t(&g(s)));
        local_grow::<Seed, N2>(inner)
    }

    fn apply_treeish_xform<N2: 'static>(
        t: &Self::TreeishXform<N2>,
        g: <Self as Domain<N>>::Graph<N>,
    ) -> <Self as Domain<N2>>::Graph<N2>
    where Self: Domain<N2>,
    {
        let new_edgy: Edgy<N2, N2> = t(&g);
        local_graph::<N2>(new_edgy)
    }

    fn apply_fold_xform<H, R, N2, H2, R2>(
        t: &Self::FoldXform<H, R, N2, H2, R2>,
        f: <Self as Domain<N>>::Fold<H, R>,
    ) -> <Self as Domain<N2>>::Fold<H2, R2>
    where Self: Domain<N2>,
          H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static,
    {
        let new_fold: Fold<N2, H2, R2> = t(f);
        local_fold::<N2, H2, R2>(new_fold)
    }

    fn identity_grow_xform() -> Self::GrowXform<N>
    where N: Clone,
    {
        Rc::new(|n: &N| n.clone())
    }

    fn identity_treeish_xform() -> Self::TreeishXform<N>
    where N: Clone,
    {
        Rc::new(|g: &Edgy<N, N>| g.clone())
    }

    fn identity_fold_xform<H: 'static, R: 'static>() -> Self::FoldXform<H, R, N, H, R> {
        Rc::new(|f: Fold<N, H, R>| f)
    }
}
