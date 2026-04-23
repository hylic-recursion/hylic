//! Formatter-fold helpers for `ExplainerDescribe`.
//!
//! These are ready-to-pass `Fn() -> Fold<ExplainerHeap<N, H, R>,
//! String, String>` constructors. Users supply one to
//! `Shared::explainer_describe_lift(fmt_ctor, emit)`; it's invoked
//! per-node at finalize to render the trace.
//!
//! `trace_fold_compact`: single-line per node, node-first then
//! child-results formatted via `{:?}`.
//!
//! `trace_fold_full`: multi-line; includes all transitions with
//! per-step `resulting_heap` snapshots.

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use std::fmt::Debug;
use crate::domain::shared::fold::{self as sfold, Fold};
use crate::prelude::explainer::ExplainerHeap;

pub fn trace_fold_compact<N, H, R>() -> Fold<ExplainerHeap<N, H, R>, String, String>
where N: Clone + Debug + Send + Sync + 'static,
      H: Clone + Debug + Send + Sync + 'static,
      R: Clone + Debug + Send + Sync + 'static,
{
    sfold::fold(
        |heap: &ExplainerHeap<N, H, R>| {
            let n = heap.transitions.len();
            format!("{:?} ({n} children) => {:?}",
                heap.node, heap.working_heap)
        },
        |_h: &mut String, _c: &String| { /* formatter is leaf per node */ },
        |h: &String| h.clone(),
    )
}

pub fn trace_fold_full<N, H, R>() -> Fold<ExplainerHeap<N, H, R>, String, String>
where N: Clone + Debug + Send + Sync + 'static,
      H: Clone + Debug + Send + Sync + 'static,
      R: Clone + Debug + Send + Sync + 'static,
{
    sfold::fold(
        |heap: &ExplainerHeap<N, H, R>| {
            let mut s = format!("node={:?}\n  initial_heap={:?}\n",
                heap.node, heap.initial_heap);
            for (i, step) in heap.transitions.iter().enumerate() {
                s.push_str(&format!(
                    "  step[{i}]: in={:?}, after={:?}\n",
                    step.incoming_result, step.resulting_heap,
                ));
            }
            s.push_str(&format!("  final_heap={:?}", heap.working_heap));
            s
        },
        |_h: &mut String, _c: &String| {},
        |h: &String| h.clone(),
    )
}

pub fn trace_fold_brief<N, H, R>() -> Fold<ExplainerHeap<N, H, R>, String, String>
where N: Clone + Debug + Send + Sync + 'static,
      H: Clone + Debug + Send + Sync + 'static,
      R: Clone + Debug + Send + Sync + 'static,
{
    sfold::fold(
        |heap: &ExplainerHeap<N, H, R>| format!("{:?} => {:?}", heap.node, heap.working_heap),
        |_h: &mut String, _c: &String| {},
        |h: &String| h.clone(),
    )
}

pub fn trace_fold_indented<N, H, R>(depth: usize) -> Fold<ExplainerHeap<N, H, R>, String, String>
where N: Clone + Debug + Send + Sync + 'static,
      H: Clone + Debug + Send + Sync + 'static,
      R: Clone + Debug + Send + Sync + 'static,
{
    let pad = "  ".repeat(depth);
    sfold::fold(
        move |heap: &ExplainerHeap<N, H, R>| {
            let mut s = format!("{pad}{:?}\n", heap.node);
            for (i, step) in heap.transitions.iter().enumerate() {
                s.push_str(&format!(
                    "{pad}  [{i}] in={:?}, after={:?}\n",
                    step.incoming_result, step.resulting_heap,
                ));
            }
            s.push_str(&format!("{pad}→ {:?}", heap.working_heap));
            s
        },
        |_h: &mut String, _c: &String| {},
        |h: &String| h.clone(),
    )
}
