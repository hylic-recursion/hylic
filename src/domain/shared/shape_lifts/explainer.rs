//! Explainer and ExplainerDescribe shape-lifts for the Shared domain.
//!
//! Distinct from the generic primitives because their MapH and MapR
//! shapes (ExplainerHeap / ExplainerResult) are specific; naming them
//! is documentation, not duplication.

use std::sync::Arc;

use crate::domain::shared::fold::{self as sfold, Fold};
use crate::domain::Shared;
use crate::ops::lift::capability::ShapeCapable;
use crate::ops::lift::shape::ShapeLift;
use crate::prelude::explainer::{ExplainerHeap, ExplainerResult, ExplainerStep};

/// Run the user-supplied formatter fold on a single ExplainerHeap,
/// producing its string form. Used by `explainer_describe_lift`.
fn run_formatter<N, H, R>(
    fmt_fold: &Fold<ExplainerHeap<N, H, R>, String, String>,
    heap: &ExplainerHeap<N, H, R>,
) -> String
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    let s = fmt_fold.init(heap);
    fmt_fold.finalize(&s)
}

impl Shared {
    /// Streaming-describe explainer: emits a rendered trace string
    /// via `emit` at each node's finalize; downstream sees `R`
    /// unchanged (MapR = R, MapH = ExplainerHeap<N, H, R>).
    pub fn explainer_describe_lift<N, H, R, FmtFold, Emit>(
        fmt_fold_ctor: FmtFold,
        emit:          Emit,
    ) -> ShapeLift<Shared, N, H, R, N, ExplainerHeap<N, H, R>, R>
    where N: Clone + Send + Sync + 'static,
          H: Clone + Send + Sync + 'static,
          R: Clone + Send + Sync + 'static,
          FmtFold: Fn() -> Fold<ExplainerHeap<N, H, R>, String, String>
                   + Send + Sync + 'static,
          Emit:    Fn(&str) + Send + Sync + 'static,
    {
        let ctor = Arc::new(fmt_fold_ctor);
        let emit = Arc::new(emit);
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<
            H, R, N, ExplainerHeap<N, H, R>, R,
        > = Arc::new(move |f: Fold<N, H, R>| {
            let f_init = f.clone();
            let f_acc  = f.clone();
            let f_fin  = f;
            let ctor   = ctor.clone();
            let emit   = emit.clone();
            sfold::fold(
                move |n: &N| ExplainerHeap::new(n.clone(), f_init.init(n)),
                move |heap: &mut ExplainerHeap<N, H, R>, child: &R| {
                    f_acc.accumulate(&mut heap.working_heap, child);
                    heap.transitions.push(ExplainerStep {
                        incoming_result: child.clone(),
                        resulting_heap:  heap.working_heap.clone(),
                    });
                },
                move |heap: &ExplainerHeap<N, H, R>| -> R {
                    let fmt_fold = ctor();
                    let s = run_formatter(&fmt_fold, heap);
                    emit(&s);
                    f_fin.finalize(&heap.working_heap)
                },
            )
        });
        // SeedSource-path note: explainer's MapH carries N, which is
        // not available at the synthetic Entry. The identity-style
        // entry_heap_xform below panics if invoked on a SeedPath;
        // users needing explainer on SeedPipelines should supply a
        // seed-specific variant with an explicit Entry-N provider
        // (follow-up; tracked in the project-entry-refactor plan).
        let entry_heap_xform: <Shared as ShapeCapable<N>>::EntryHeapXform<H, ExplainerHeap<N, H, R>> =
            Arc::new(|_h: H| panic!(
                "explainer_describe_lift::project_entry_heap is not usable on SeedSource \
                 (MapH = ExplainerHeap<N,H,R> requires an N at Entry). \
                 Use a seed-specific constructor when needed."));
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
            <Shared as ShapeCapable<N>>::identity_entry_node_xform(),
            entry_heap_xform,
        )
    }

    /// Whole-tree Explainer: MapH = ExplainerHeap wrapping the
    /// nested ExplainerResult; MapR = ExplainerResult capturing the
    /// full trace.
    // ANCHOR: explainer_lift_ctor
    pub fn explainer_lift<N, H, R>()
        -> ShapeLift<Shared, N, H, R,
                     N,
                     ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
                     ExplainerResult<N, H, R>>
    where N: Clone + Send + Sync + 'static,
          H: Clone + Send + Sync + 'static,
          R: Clone + Send + Sync + 'static,
    {
        let fold_xform: <Shared as ShapeCapable<N>>::FoldXform<
            H, R, N,
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
        // See note on explainer_describe_lift: MapH carries N.
        let entry_heap_xform: <Shared as ShapeCapable<N>>::EntryHeapXform<
            H, ExplainerHeap<N, H, ExplainerResult<N, H, R>>,
        > = Arc::new(|_h: H| panic!(
            "explainer_lift::project_entry_heap is not usable on SeedSource \
             (MapH = ExplainerHeap<...> requires an N at Entry). \
             Use a seed-specific constructor when needed."));
        ShapeLift::new(
            <Shared as ShapeCapable<N>>::identity_treeish_xform(),
            fold_xform,
            <Shared as ShapeCapable<N>>::identity_entry_node_xform(),
            entry_heap_xform,
        )
    }
    // ANCHOR_END: explainer_lift_ctor
}
