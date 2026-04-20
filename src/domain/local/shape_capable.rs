//! `impl ShapeCapable<N> for Local` — Rc-backed xform storage,
//! no Send+Sync bound. Enables `ShapeLift<Local, …>`.

use std::rc::Rc;
use crate::domain::{Domain, Local};
use crate::domain::local::Fold;
use crate::domain::local::edgy::Edgy;
use crate::ops::lift::capability::ShapeCapable;

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
        g: Rc<dyn Fn(&Seed) -> N>,
    ) -> Rc<dyn Fn(&Seed) -> N2>
    where Self: Domain<N2>,
    {
        let t = t.clone();
        Rc::new(move |s: &Seed| t(&g(s)))
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
