// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Fold-side Local sugars — one-line wrappers over
//! `Local::phases_lift`.

use std::rc::Rc;

use crate::domain::Local;
use crate::ops::lift::shape::universal::ShapeLift;

impl Local {
    pub fn wrap_init_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Local, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&N, &dyn Fn(&N) -> H) -> H + 'static,
    {
        let w = Rc::new(wrapper);
        let mi = move |old: Rc<dyn Fn(&N) -> H>| -> Rc<dyn Fn(&N) -> H> {
            let w = w.clone();
            Rc::new(move |n: &N| w(n, &*old))
        };
        Local::phases_lift::<N, H, R, H, R, _, _, _>(
            mi,
            Local::identity_acc_mapper::<H, R>(),
            Local::identity_fin_mapper::<H, R>(),
        )
    }

    pub fn wrap_accumulate_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Local, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + 'static,
    {
        let w = Rc::new(wrapper);
        let ma = move |old: Rc<dyn Fn(&mut H, &R)>| -> Rc<dyn Fn(&mut H, &R)> {
            let w = w.clone();
            Rc::new(move |h: &mut H, r: &R| w(h, r, &*old))
        };
        Local::phases_lift::<N, H, R, H, R, _, _, _>(
            Local::identity_init_mapper::<N, H>(),
            ma,
            Local::identity_fin_mapper::<H, R>(),
        )
    }

    pub fn wrap_finalize_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Local, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&H, &dyn Fn(&H) -> R) -> R + 'static,
    {
        let w = Rc::new(wrapper);
        let mf = move |old: Rc<dyn Fn(&H) -> R>| -> Rc<dyn Fn(&H) -> R> {
            let w = w.clone();
            Rc::new(move |h: &H| w(h, &*old))
        };
        Local::phases_lift::<N, H, R, H, R, _, _, _>(
            Local::identity_init_mapper::<N, H>(),
            Local::identity_acc_mapper::<H, R>(),
            mf,
        )
    }

    pub fn zipmap_lift<N, H, R, Extra, M>(mapper: M)
        -> ShapeLift<Local, N, H, R, N, H, (R, Extra)>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        Extra: Clone + 'static,
        M: Fn(&R) -> Extra + 'static,
    {
        let m = Rc::new(mapper);
        let ma = Rc::new(move |old: Rc<dyn Fn(&mut H, &R)>|
                           -> Rc<dyn Fn(&mut H, &(R, Extra))>
        {
            Rc::new(move |h: &mut H, r: &(R, Extra)| old(h, &r.0))
        });
        let mf = {
            let m = m.clone();
            Rc::new(move |old: Rc<dyn Fn(&H) -> R>|
                       -> Rc<dyn Fn(&H) -> (R, Extra)>
            {
                let m = m.clone();
                Rc::new(move |h: &H| {
                    let r = old(h);
                    let extra = m(&r);
                    (r, extra)
                })
            })
        };
        Local::phases_lift::<N, H, R, H, (R, Extra), _, _, _>(
            Local::identity_init_mapper::<N, H>(),
            move |old| (ma)(old),
            move |old| (mf)(old),
        )
    }

    pub fn map_r_bi_lift<N, H, R, RNew, Fwd, Bwd>(forward: Fwd, backward: Bwd)
        -> ShapeLift<Local, N, H, R, N, H, RNew>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        RNew: Clone + 'static,
        Fwd: Fn(&R) -> RNew + 'static,
        Bwd: Fn(&RNew) -> R + 'static,
    {
        let fwd = Rc::new(forward);
        let bwd = Rc::new(backward);
        let ma = {
            let bwd = bwd.clone();
            Rc::new(move |old: Rc<dyn Fn(&mut H, &R)>|
                       -> Rc<dyn Fn(&mut H, &RNew)>
            {
                let bwd = bwd.clone();
                Rc::new(move |h: &mut H, r: &RNew| old(h, &bwd(r)))
            })
        };
        let mf = {
            let fwd = fwd.clone();
            Rc::new(move |old: Rc<dyn Fn(&H) -> R>|
                       -> Rc<dyn Fn(&H) -> RNew>
            {
                let fwd = fwd.clone();
                Rc::new(move |h: &H| fwd(&old(h)))
            })
        };
        Local::phases_lift::<N, H, R, H, RNew, _, _, _>(
            Local::identity_init_mapper::<N, H>(),
            move |old| (ma)(old),
            move |old| (mf)(old),
        )
    }
}
