//! Explainer shape-lift for the Local domain.
//! (ExplainerDescribe deferred — the formatter-fold run requires
//! Shared for the fmt_fold_ctor's Send+Sync bound; Local variant
//! can ship when the build_explainer_fold helper goes
//! domain-generic.)

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use std::rc::Rc;

use crate::domain::local::{self, Fold};
use crate::domain::Local;
use crate::ops::lift::capability::ShapeCapable;
use crate::ops::lift::shape::ShapeLift;
use crate::prelude::explainer::{ExplainerHeap, ExplainerResult, ExplainerStep};

impl Local {
    pub fn explainer_lift<N, H, R>()
        -> ShapeLift<Local, N, H, R,
                     N,
                     ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
                     ExplainerResult<N, H, R>>
    where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
    {
        let fold_xform: <Local as ShapeCapable<N>>::FoldXform<
            H, R, N,
            ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
            ExplainerResult<N, H, R>,
        > = Rc::new(move |f: Fold<N, H, R>| {
            let f1 = f.clone();
            let f2 = f.clone();
            let f3 = f;
            local::fold(
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
            <Local as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
        )
    }
}
