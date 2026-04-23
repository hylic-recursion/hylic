//! Local-domain general primitives. Mirror of Shared with Rc.

use std::rc::Rc;

use crate::domain::local::Fold;
use crate::domain::local::edgy::Edgy;
use crate::domain::Local;
use crate::ops::lift::capability::ShapeCapable;
use crate::ops::lift::shape::ShapeLift;

impl Local {
    pub(crate) fn identity_init_mapper<N, H>()
        -> impl Fn(Rc<dyn Fn(&N) -> H>) -> Rc<dyn Fn(&N) -> H> + Clone + 'static
    where N: 'static, H: 'static,
    { |init| init }

    pub(crate) fn identity_acc_mapper<H, R>()
        -> impl Fn(Rc<dyn Fn(&mut H, &R)>) -> Rc<dyn Fn(&mut H, &R)> + Clone + 'static
    where H: 'static, R: 'static,
    { |acc| acc }

    pub(crate) fn identity_fin_mapper<H, R>()
        -> impl Fn(Rc<dyn Fn(&H) -> R>) -> Rc<dyn Fn(&H) -> R> + Clone + 'static
    where H: 'static, R: 'static,
    { |fin| fin }
}

impl Local {
    pub fn phases_lift<N, H, R, NewH, NewR, MI, MA, MF>(
        mi: MI, ma: MA, mf: MF,
    ) -> ShapeLift<Local, N, H, R, N, NewH, NewR>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        NewH: Clone + 'static, NewR: Clone + 'static,
        MI: Fn(Rc<dyn Fn(&N) -> H>) -> Rc<dyn Fn(&N) -> NewH> + 'static,
        MA: Fn(Rc<dyn Fn(&mut H, &R)>) -> Rc<dyn Fn(&mut NewH, &NewR)> + 'static,
        MF: Fn(Rc<dyn Fn(&H) -> R>) -> Rc<dyn Fn(&NewH) -> NewR> + 'static,
    {
        let mi = Rc::new(mi);
        let ma = Rc::new(ma);
        let mf = Rc::new(mf);
        let fold_xform: <Local as ShapeCapable<N>>::FoldXform<H, R, N, NewH, NewR> =
            Rc::new(move |f: Fold<N, H, R>| -> Fold<N, NewH, NewR> {
                let mi = mi.clone();
                let ma = ma.clone();
                let mf = mf.clone();
                use crate::ops::FoldTransformsByRef;
                <Fold<N, H, R> as FoldTransformsByRef<N, H, R>>::map_phases::<N, NewH, NewR, _, _, _>(
                    &f,
                    move |init| (mi)(init),
                    move |acc|  (ma)(acc),
                    move |fin|  (mf)(fin),
                )
            });
        ShapeLift::new(
            <Local as ShapeCapable<N>>::identity_grow_xform(),
            <Local as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }
}

impl Local {
    pub fn treeish_lift<N, H, R, MT>(mt: MT) -> ShapeLift<Local, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        MT: Fn(Edgy<N, N>) -> Edgy<N, N> + 'static,
    {
        let mt = Rc::new(mt);
        let treeish_xform: <Local as ShapeCapable<N>>::TreeishXform<N> = {
            let mt = mt.clone();
            Rc::new(move |g: &Edgy<N, N>| (mt)(g.clone()))
        };
        ShapeLift::new(
            <Local as ShapeCapable<N>>::identity_grow_xform(),
            treeish_xform,
            <Local as ShapeCapable<N>>::identity_fold_xform::<H, R>(),
        )
    }
}

impl Local {
    pub fn n_lift<N, H, R, N2, LN, BT, FC>(
        lift_node:     LN,
        build_treeish: BT,
        fold_contra:   FC,
    ) -> ShapeLift<Local, N, H, R, N2, H, R>
    where
        N:  Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        N2: Clone + 'static,
        LN: Fn(&N) -> N2 + 'static,
        BT: Fn(&Edgy<N, N>) -> Edgy<N2, N2> + 'static,
        FC: Fn(&N2) -> N + 'static,
    {
        let grow_xform:    <Local as ShapeCapable<N>>::GrowXform<N2>    = Rc::new(lift_node);
        let treeish_xform: <Local as ShapeCapable<N>>::TreeishXform<N2> = Rc::new(build_treeish);
        let fold_xform:    <Local as ShapeCapable<N>>::FoldXform<H, R, N2, H, R> = {
            let fc = Rc::new(fold_contra);
            Rc::new(move |f: Fold<N, H, R>| {
                let fc = fc.clone();
                f.contramap_n(move |n2: &N2| fc(n2))
            })
        };
        ShapeLift::new(grow_xform, treeish_xform, fold_xform)
    }
}
