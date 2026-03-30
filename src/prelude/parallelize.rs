//! UIO-based fold parallelization as a Lift.
//!
//! Transforms the fold so each node's result is a deferred computation
//! (UIO<R>). Sibling subtrees evaluate in parallel via join_par.
//! The Treeish is unchanged — structure traversal is separate from
//! parallelism. The executor runs the lifted computation with fused
//! traversal; parallelism lives entirely in the UIO closures.

use crate::cata::Lift;
use crate::uio::UIO;
use crate::prelude::vec_fold::{vec_fold, VecHeap};

/// Create a Lift that parallelizes fold execution via UIO.
///
/// The lifted fold produces `UIO<R>` instead of `R`. When the
/// computation is unwrapped (via Lift::unwrap / Exec::run_lifted),
/// `UIO::eval()` triggers parallel evaluation of sibling subtrees.
pub fn uio_parallel<N, H, R>() -> Lift<N, H, R, N, VecHeap<N, UIO<R>>, UIO<R>>
where
    N: Clone + Send + Sync + 'static,
    H: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    Lift::new(
        // lift_treeish: identity — tree structure unchanged
        |treeish| treeish,

        // lift_fold: wrap the fold to produce UIO<R> results
        move |fold| {
            vec_fold(move |heap: &VecHeap<N, UIO<R>>| {
                let node = heap.node.clone();
                let child_uios: Vec<UIO<R>> = heap.childresults.clone();
                let fold = fold.clone();
                UIO::new(move || {
                    let mut h = fold.init(&node);
                    if child_uios.len() <= 1 {
                        for uio in &child_uios {
                            fold.accumulate(&mut h, uio.eval());
                        }
                    } else {
                        for r in UIO::join_par(child_uios).eval() {
                            fold.accumulate(&mut h, &r);
                        }
                    }
                    fold.finalize(&h)
                })
            })
        },

        // lift_root: identity (N2 = N)
        |n: &N| n.clone(),

        // unwrap: evaluate the deferred computation
        |uio: UIO<R>| uio.eval().clone(),
    )
}
