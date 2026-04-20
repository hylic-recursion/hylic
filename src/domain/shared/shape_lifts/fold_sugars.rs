// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Fold-side Shared sugars — one-line wrappers over
//! `Shared::phases_lift`.

use std::sync::Arc;

use crate::domain::Shared;
use crate::ops::lift::shape::universal::ShapeLift;

// ── wrap_init / wrap_accumulate / wrap_finalize — compose-with-old ─────

impl Shared {
    pub fn wrap_init_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    {
        let w = Arc::new(wrapper);
        let mi = move |old: Arc<dyn Fn(&N) -> H + Send + Sync>|
                   -> Arc<dyn Fn(&N) -> H + Send + Sync> {
            let w = w.clone();
            Arc::new(move |n: &N| w(n, &*old))
        };
        Shared::phases_lift::<N, H, R, H, R, _, _, _>(
            mi,
            Shared::identity_acc_mapper::<H, R>(),
            Shared::identity_fin_mapper::<H, R>(),
        )
    }

    pub fn wrap_accumulate_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    {
        let w = Arc::new(wrapper);
        let ma = move |old: Arc<dyn Fn(&mut H, &R) + Send + Sync>|
                   -> Arc<dyn Fn(&mut H, &R) + Send + Sync> {
            let w = w.clone();
            Arc::new(move |h: &mut H, r: &R| w(h, r, &*old))
        };
        Shared::phases_lift::<N, H, R, H, R, _, _, _>(
            Shared::identity_init_mapper::<N, H>(),
            ma,
            Shared::identity_fin_mapper::<H, R>(),
        )
    }

    pub fn wrap_finalize_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    {
        let w = Arc::new(wrapper);
        let mf = move |old: Arc<dyn Fn(&H) -> R + Send + Sync>|
                   -> Arc<dyn Fn(&H) -> R + Send + Sync> {
            let w = w.clone();
            Arc::new(move |h: &H| w(h, &*old))
        };
        Shared::phases_lift::<N, H, R, H, R, _, _, _>(
            Shared::identity_init_mapper::<N, H>(),
            Shared::identity_acc_mapper::<H, R>(),
            mf,
        )
    }
}

// ── zipmap / map_r — R-axis changes ────────────────────────────────────

impl Shared {
    pub fn zipmap_lift<N, H, R, Extra, M>(mapper: M)
        -> ShapeLift<Shared, N, H, R, N, H, (R, Extra)>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        Extra: Clone + 'static,
        M: Fn(&R) -> Extra + Send + Sync + 'static,
    {
        let m = Arc::new(mapper);
        // New accumulate: given the new R = (R, Extra), delegate to
        // the old accumulate with the R component.
        let ma = {
            Arc::new(move |old: Arc<dyn Fn(&mut H, &R) + Send + Sync>|
                       -> Arc<dyn Fn(&mut H, &(R, Extra)) + Send + Sync>
            {
                Arc::new(move |h: &mut H, r: &(R, Extra)| old(h, &r.0))
            })
        };
        // New finalize: pair (R, Extra).
        let mf = {
            let m = m.clone();
            Arc::new(move |old: Arc<dyn Fn(&H) -> R + Send + Sync>|
                       -> Arc<dyn Fn(&H) -> (R, Extra) + Send + Sync>
            {
                let m = m.clone();
                Arc::new(move |h: &H| {
                    let r = old(h);
                    let extra = m(&r);
                    (r, extra)
                })
            })
        };
        Shared::phases_lift::<N, H, R, H, (R, Extra), _, _, _>(
            Shared::identity_init_mapper::<N, H>(),
            move |old| (ma)(old),
            move |old| (mf)(old),
        )
    }

    pub fn map_r_bi_lift<N, H, R, RNew, Fwd, Bwd>(forward: Fwd, backward: Bwd)
        -> ShapeLift<Shared, N, H, R, N, H, RNew>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        RNew: Clone + 'static,
        Fwd: Fn(&R) -> RNew + Send + Sync + 'static,
        Bwd: Fn(&RNew) -> R + Send + Sync + 'static,
    {
        let fwd = Arc::new(forward);
        let bwd = Arc::new(backward);
        let ma = {
            let bwd = bwd.clone();
            Arc::new(move |old: Arc<dyn Fn(&mut H, &R) + Send + Sync>|
                       -> Arc<dyn Fn(&mut H, &RNew) + Send + Sync>
            {
                let bwd = bwd.clone();
                Arc::new(move |h: &mut H, r: &RNew| old(h, &bwd(r)))
            })
        };
        let mf = {
            let fwd = fwd.clone();
            Arc::new(move |old: Arc<dyn Fn(&H) -> R + Send + Sync>|
                       -> Arc<dyn Fn(&H) -> RNew + Send + Sync>
            {
                let fwd = fwd.clone();
                Arc::new(move |h: &H| fwd(&old(h)))
            })
        };
        Shared::phases_lift::<N, H, R, H, RNew, _, _, _>(
            Shared::identity_init_mapper::<N, H>(),
            move |old| (ma)(old),
            move |old| (mf)(old),
        )
    }
}
