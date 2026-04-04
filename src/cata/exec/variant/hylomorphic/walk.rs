//! walk + dispatch_rest: the CPS recursion engine.

use crate::ops::{FoldOps, TreeOps, ChildCursor};
use crate::prelude::parallel::pool::{PoolExecView, SyncRef};
use super::slot_chain::SlotChain;

pub fn walk<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    node: N,
    view: &PoolExecView,
) -> R
where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let heap = fold.init(&node);

    match graph.first_child(&node) {
        None => fold.finalize(&heap),
        Some((first_child, cursor)) => {
            let chain = SlotChain::new(heap);
            let slot_0 = chain.append_slot();

            let (r0, ()) = view.join(
                || walk(fold, graph, first_child, view),
                || dispatch_rest(fold, graph, cursor, &chain, view),
            );

            chain.deliver::<N>(slot_0, r0, fold.0);
            chain.finish::<N>(fold.0)
        }
    }
}

fn dispatch_rest<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    cursor: ChildCursor<N>,
    chain: &SlotChain<H, R>,
    view: &PoolExecView,
) where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    match cursor.next() {
        None => {
            chain.set_total::<N>(fold.0);
        }
        Some((child, next_cursor)) => {
            let slot = chain.append_slot();

            let (r, ()) = view.join(
                || walk(fold, graph, child, view),
                || dispatch_rest(fold, graph, next_cursor, chain, view),
            );

            chain.deliver::<N>(slot, r, fold.0);
        }
    }
}

pub fn run_fold<N, H, R>(
    fold: &impl FoldOps<N, H, R>,
    graph: &impl TreeOps<N>,
    root: &N,
    view: &PoolExecView,
) -> R
where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let sf = SyncRef(fold);
    let sg = SyncRef(graph);
    walk(&sf, &sg, root.clone(), view)
}
