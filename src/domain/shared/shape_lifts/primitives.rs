//! Shared-domain general primitives.
//!
//! Three primitives, each rewriting at most a coordinated subset of
//! the (grow, treeish, fold) triple:
//!
//!   - `map_fold_phases_lift(mi, ma, mf)` — fold-phase rewrite
//!   - `map_treeish_lift(mt)`              — treeish rewrite
//!   - `inline_lift(ln, bt, fc)`           — N-change with all three
//!
//! Every other Shared shape-lift (wrap_init, zipmap, map_r,
//! contramap_n, filter_edges, wrap_visit, memoize_by, …) is a thin
//! wrapper over one of these three.

use std::sync::Arc;

use crate::domain::shared::fold::Fold;
use crate::domain::Shared;
use crate::graph::Edgy;
use crate::ops::lift::capability::ShapeCapable;
use crate::ops::lift::shape::universal::ShapeLift;

// ── Identity phase mappers (usable when the corresponding type axis is preserved) ─

impl Shared {
    /// Identity mapper for the init phase. Valid when `NewH = H`.
    pub fn identity_init_mapper<N, H>()
        -> impl Fn(Arc<dyn Fn(&N) -> H + Send + Sync>)
               -> Arc<dyn Fn(&N) -> H + Send + Sync>
            + Send + Sync + Clone + 'static
    where N: 'static, H: 'static,
    {
        |init| init
    }

    /// Identity mapper for the accumulate phase. Valid when
    /// `NewH = H` AND `NewR = R`.
    pub fn identity_acc_mapper<H, R>()
        -> impl Fn(Arc<dyn Fn(&mut H, &R) + Send + Sync>)
               -> Arc<dyn Fn(&mut H, &R) + Send + Sync>
            + Send + Sync + Clone + 'static
    where H: 'static, R: 'static,
    {
        |acc| acc
    }

    /// Identity mapper for the finalize phase. Valid when
    /// `NewH = H` AND `NewR = R`.
    pub fn identity_fin_mapper<H, R>()
        -> impl Fn(Arc<dyn Fn(&H) -> R + Send + Sync>)
               -> Arc<dyn Fn(&H) -> R + Send + Sync>
            + Send + Sync + Clone + 'static
    where H: 'static, R: 'static,
    {
        |fin| fin
    }
}

// ── map_fold_phases_lift — fold-phase rewrite primitive ────────────────

impl Shared {
    pub fn map_fold_phases_lift<N, H, R, NewH, NewR, MI, MA, MF>(
        mi: MI, ma: MA, mf: MF,
    ) -> ShapeLift<Shared, N, H, R, N, NewH, NewR>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        NewH: Clone + 'static, NewR: Clone + 'static,
        MI: Fn(Arc<dyn Fn(&N) -> H + Send + Sync>)
            -> Arc<dyn Fn(&N) -> NewH + Send + Sync>
            + Send + Sync + 'static,
        MA: Fn(Arc<dyn Fn(&mut H, &R) + Send + Sync>)
            -> Arc<dyn Fn(&mut NewH, &NewR) + Send + Sync>
            + Send + Sync + 'static,
        MF: Fn(Arc<dyn Fn(&H) -> R + Send + Sync>)
            -> Arc<dyn Fn(&NewH) -> NewR + Send + Sync>
            + Send + Sync + 'static,
    {
        let mi = Arc::new(mi);
        let ma = Arc::new(ma);
        let mf = Arc::new(mf);
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<H, R, N, NewH, NewR> =
            Arc::new(move |f: Fold<N, H, R>| -> Fold<N, NewH, NewR> {
                let mi = mi.clone();
                let ma = ma.clone();
                let mf = mf.clone();
                // FoldTransformsByRef::map_phases takes FnOnce on each
                // mapper. We hold Arc<Fn>; wrapping in a move closure
                // produces the FnOnce the trait wants.
                use crate::ops::FoldTransformsByRef;
                <Fold<N, H, R> as FoldTransformsByRef<N, H, R>>::map_phases::<N, NewH, NewR, _, _, _>(
                    &f,
                    move |init| (mi)(init),
                    move |acc|  (ma)(acc),
                    move |fin|  (mf)(fin),
                )
            });
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }
}

// ── map_treeish_lift — treeish rewrite primitive ───────────────────────

impl Shared {
    pub fn map_treeish_lift<N, H, R, MT>(
        mt: MT,
    ) -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        MT: Fn(Edgy<N, N>) -> Edgy<N, N> + Send + Sync + 'static,
    {
        let mt = Arc::new(mt);
        let treeish_xform: <Shared as ShapeCapable<N>>::TreeishXform<N> = {
            let mt = mt.clone();
            Arc::new(move |g: &Edgy<N, N>| (mt)(g.clone()))
        };
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            treeish_xform,
            <Shared as ShapeCapable<N>>::identity_fold_xform::<H, R>(),
        )
    }
}

// ── inline_lift — N-change primitive (coordinated grow + treeish + fold) ─

impl Shared {
    pub fn inline_lift<N, H, R, N2, LN, BT, FC>(
        lift_node:     LN,
        build_treeish: BT,
        fold_contra:   FC,
    ) -> ShapeLift<Shared, N, H, R, N2, H, R>
    where
        N:  Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        N2: Clone + 'static,
        LN: Fn(&N) -> N2 + Send + Sync + 'static,
        BT: Fn(&Edgy<N, N>) -> Edgy<N2, N2> + Send + Sync + 'static,
        FC: Fn(&N2) -> N + Send + Sync + 'static,
    {
        let grow_xform:    <Shared as ShapeCapable<N>>::GrowXform<N2>    = Arc::new(lift_node);
        let treeish_xform: <Shared as ShapeCapable<N>>::TreeishXform<N2> = Arc::new(build_treeish);
        let fold_xform:    <Shared as ShapeCapable<N>>::FoldXform<H, R, N2, H, R> = {
            let fc = Arc::new(fold_contra);
            Arc::new(move |f: Fold<N, H, R>| {
                let fc = fc.clone();
                f.contramap(move |n2: &N2| fc(n2))
            })
        };
        ShapeLift::new(grow_xform, treeish_xform, fold_xform)
    }
}

