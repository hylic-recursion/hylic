//! UIO-based fold parallelization as a Lift.
//!
//! Transforms the fold so each node's result is a deferred computation
//! (UIO<R>). Sibling subtrees evaluate in parallel via join_par.
//! The Treeish is unchanged — structure traversal is separate from
//! parallelism.
//!
//! Zero node clones: &N is consumed by fold.init within the callback
//! (producing owned H). The UIO closure captures only the heap and
//! child UIO handles — never the node itself.

use crate::cata::Lift;
use crate::fold;
use crate::uio::UIO;

/// Create a Lift that parallelizes fold execution via UIO.
///
/// The lifted fold stores (H, Vec<UIO<R>>) as its heap:
/// - init: calls the original fold's init(&N) → owned H. Last use of &N.
/// - accumulate: collects child UIO<R> handles (Arc bump each, O(1))
/// - finalize: packages into a UIO that, when evaluated, runs join_par
///   on children and accumulates their results.
///
/// No node clone. H is cloned once in finalize (unavoidable — finalize
/// borrows the heap but UIO needs owned data).
pub fn uio_parallel<N, H, R>() -> Lift<N, H, R, N, (H, Vec<UIO<R>>), UIO<R>>
where
    N: Clone + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    Lift::new(
        // lift_treeish: identity — tree structure unchanged
        |treeish| treeish,

        // lift_fold: Fold<N, H, R> → Fold<N, (H, Vec<UIO<R>>), UIO<R>>
        move |original_fold| {
            let f_init = original_fold.clone();
            let f_fin = original_fold.clone();
            fold::fold(
                // init: consume &N into owned H. No node stored.
                move |node: &N| -> (H, Vec<UIO<R>>) {
                    (f_init.init(node), Vec::new())
                },
                // accumulate: collect child UIO handles. O(1) per child.
                |state: &mut (H, Vec<UIO<R>>), child_uio: &UIO<R>| {
                    state.1.push(child_uio.clone()); // Arc bump
                },
                // finalize: package deferred computation.
                move |state: &(H, Vec<UIO<R>>)| -> UIO<R> {
                    let heap = state.0.clone();       // clone H (not N)
                    let children = state.1.clone();   // clone Vec<UIO> (Arc bumps)
                    let fold = f_fin.clone();         // Arc bumps
                    UIO::new(move || {
                        let mut h = heap;
                        if children.len() <= 1 {
                            for uio in &children {
                                fold.accumulate(&mut h, uio.eval());
                            }
                        } else {
                            for r in UIO::join_par(children).eval() {
                                fold.accumulate(&mut h, &r);
                            }
                        }
                        fold.finalize(&h)
                    })
                },
            )
        },

        // lift_root: identity (N2 = N, no clone needed — just borrow)
        |n: &N| n.clone(),

        // unwrap: evaluate the deferred computation
        |uio: UIO<R>| uio.eval().clone(),
    )
}
