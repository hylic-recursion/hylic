//! Shape-lift constructor functions on the `Shared` domain.
//!
//! Each constructor produces a `ShapeLift<Shared, …>` with
//! domain-natural Arc+Send+Sync storage. The ShapeLift's Lift impl
//! is polymorphic over D (see `ops/lift/shape/universal.rs`); the
//! per-domain specificity lives entirely in these constructor bodies.

use std::sync::Arc;

use crate::domain::shared::fold::{self as sfold, Fold};
use crate::domain::Shared;
use crate::graph::{edgy_visit, Edgy};
use crate::ops::lift::capability::ShapeCapable;
use crate::ops::lift::shape::universal::ShapeLift;
use crate::prelude::explainer::{ExplainerHeap, ExplainerResult, ExplainerStep};

impl Shared {
    // ── N-preserving, H/R-preserving ─────────────────────

    pub fn wrap_init_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    {
        let w = Arc::new(wrapper);
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<H, R, N, H, R> =
            Arc::new(move |f: Fold<N, H, R>| {
                let w = w.clone();
                f.wrap_init(move |n, orig| w(n, orig))
            });
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }

    pub fn wrap_accumulate_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    {
        let w = Arc::new(wrapper);
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<H, R, N, H, R> =
            Arc::new(move |f: Fold<N, H, R>| {
                let w = w.clone();
                f.wrap_accumulate(move |h, r, orig| w(h, r, orig))
            });
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }

    pub fn wrap_finalize_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    {
        let w = Arc::new(wrapper);
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<H, R, N, H, R> =
            Arc::new(move |f: Fold<N, H, R>| {
                let w = w.clone();
                f.wrap_finalize(move |h, orig| w(h, orig))
            });
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }

    // ── N/H-preserving, R-changing ───────────────────────

    pub fn zipmap_lift<N, H, R, Extra, M>(mapper: M)
        -> ShapeLift<Shared, N, H, R, N, H, (R, Extra)>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        Extra: Clone + 'static,
        M: Fn(&R) -> Extra + Send + Sync + 'static,
    {
        let m = Arc::new(mapper);
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<H, R, N, H, (R, Extra)> =
            Arc::new(move |f: Fold<N, H, R>| {
                let m = m.clone();
                f.zipmap(move |r: &R| m(r))
            });
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }

    pub fn map_r_lift<N, H, R, RNew, Fwd, Bwd>(forward: Fwd, backward: Bwd)
        -> ShapeLift<Shared, N, H, R, N, H, RNew>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        RNew: Clone + 'static,
        Fwd: Fn(&R) -> RNew + Send + Sync + 'static,
        Bwd: Fn(&RNew) -> R + Send + Sync + 'static,
    {
        let fwd = Arc::new(forward);
        let bwd = Arc::new(backward);
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<H, R, N, H, RNew> =
            Arc::new(move |f: Fold<N, H, R>| {
                let fwd = fwd.clone();
                let bwd = bwd.clone();
                f.map(move |r: &R| fwd(r), move |r: &RNew| bwd(r))
            });
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }

    // ── N-changing (bijective), H/R-preserving ───────────

    pub fn contramap_n_lift<N, H, R, N2, Co, Contra>(co: Co, contra: Contra)
        -> ShapeLift<Shared, N, H, R, N2, H, R>
    where
        N:  Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        N2: Clone + 'static,
        Co:     Fn(&N)  -> N2 + Send + Sync + 'static,
        Contra: Fn(&N2) -> N  + Send + Sync + 'static,
    {
        let co_arc:     <Shared as ShapeCapable<N>>::GrowXform<N2> = Arc::new(co);
        let contra_arc: Arc<dyn Fn(&N2) -> N + Send + Sync>         = Arc::new(contra);

        let treeish_xform: <Shared as ShapeCapable<N>>::TreeishXform<N2> = {
            let co = co_arc.clone();
            let ca = contra_arc.clone();
            Arc::new(move |g: &Edgy<N, N>| {
                let g = g.clone();
                let co = co.clone();
                let ca = ca.clone();
                edgy_visit(move |n2: &N2, cb: &mut dyn FnMut(&N2)| {
                    let n: N = ca(n2);
                    g.visit(&n, &mut |child: &N| cb(&co(child)));
                })
            })
        };

        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<H, R, N2, H, R> = {
            let ca = contra_arc.clone();
            Arc::new(move |f: Fold<N, H, R>| {
                let ca = ca.clone();
                f.contramap(move |n2: &N2| ca(n2))
            })
        };

        ShapeLift::new(co_arc, treeish_xform, fold_xform)
    }

    // ── N-changing (context-dependent), H/R-preserving ───

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

    // ── N-preserving, H and R changing — Explainer ───────

    pub fn explainer_lift<N, H, R>()
        -> ShapeLift<
              Shared, N, H, R,
              N,
              ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
              ExplainerResult<N, H, R>,
           >
    where N: Clone + Send + Sync + 'static,
          H: Clone + Send + Sync + 'static,
          R: Clone + Send + Sync + 'static,
    {
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<
            H, R,
            N,
            ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
            ExplainerResult<N, H, R>,
        > = Arc::new(move |f: Fold<N, H, R>| {
            let f1 = f.clone();
            let f2 = f.clone();
            let f3 = f;
            sfold::fold(
                move |n: &N| ExplainerHeap::new(n.clone(), f1.init(n)),
                move |heap: &mut ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
                      child: &ExplainerResult<N, H, R>| {
                    f2.accumulate(&mut heap.working_heap, &child.orig_result);
                    heap.transitions.push(ExplainerStep {
                        incoming_result: child.clone(),
                        resulting_heap:  heap.working_heap.clone(),
                    });
                },
                move |heap: &ExplainerHeap<N, H, ExplainerResult<N, H, R>>| ExplainerResult {
                    orig_result: f3.finalize(&heap.working_heap),
                    heap:        heap.clone(),
                },
            )
        });
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_grow_xform(),
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }
}
