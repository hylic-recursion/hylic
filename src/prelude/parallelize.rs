//! Lazy ParRef-based fold parallelization as a Lift.
//!
//! Transforms the fold so each node's result is a deferred computation
//! (ParRef<R>). Sibling subtrees evaluate in parallel via join_par.
//! Zero node clones: &N consumed by fold.init within the callback.

use crate::cata::Lift;
use crate::fold;
use crate::parref::ParRef;

/// Lazy ParRef parallelization. Traversal builds the ParRef tree; eval()
/// triggers parallel bottom-up evaluation via join_par.
pub fn parref_lazy<N, H, R>() -> Lift<N, H, R, N, (H, Vec<ParRef<R>>), ParRef<R>>
where
    N: Clone + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    Lift::new(
        |treeish| treeish,
        move |original_fold| {
            let f_init = original_fold.clone();
            let f_fin = original_fold.clone();
            fold::fold(
                move |node: &N| -> (H, Vec<ParRef<R>>) {
                    (f_init.init(node), Vec::new())
                },
                |state: &mut (H, Vec<ParRef<R>>), child: &ParRef<R>| {
                    state.1.push(child.clone());
                },
                move |state: &(H, Vec<ParRef<R>>)| -> ParRef<R> {
                    let heap = state.0.clone();
                    let children = state.1.clone();
                    let fold = f_fin.clone();
                    ParRef::new(move || {
                        let mut h = heap;
                        if children.len() <= 1 {
                            for c in &children { fold.accumulate(&mut h, c.eval()); }
                        } else {
                            for r in ParRef::join_par(children).eval() {
                                fold.accumulate(&mut h, &r);
                            }
                        }
                        fold.finalize(&h)
                    })
                },
            )
        },
        |n: &N| n.clone(),
        |pr: ParRef<R>| pr.eval().clone(),
    )
}
