//! walk + dispatch_siblings: the CPS recursion engine.
//!
//! Mutually recursive. The call stack IS the zipper. join() IS the fork.

use std::sync::Arc;
use crate::ops::{FoldOps, TreeOps, ChildCursor};
use crate::prelude::parallel::pool::{PoolExecView, SyncRef};
use super::slot_chain::SlotChain;

/// What to do with a result.
pub enum Cont<H, R> {
    /// Root completed. Write result here.
    Root(std::cell::UnsafeCell<Option<R>>),
    /// Deliver to a parent's chain at a specific slot.
    Deliver {
        slot: *const super::slot_chain::Slot<R>,
        chain: Arc<ChainWithCont<H, R>>,
    },
}

unsafe impl<H: Send, R: Send> Send for Cont<H, R> {}

/// A SlotChain paired with the continuation to invoke when the chain
/// completes (all children accumulated, finalize produces a result).
pub struct ChainWithCont<H, R> {
    pub chain: SlotChain<H, R>,
    parent_cont: std::cell::UnsafeCell<Option<Cont<H, R>>>,
}

unsafe impl<H: Send, R: Send> Send for ChainWithCont<H, R> {}
unsafe impl<H: Send, R: Send> Sync for ChainWithCont<H, R> {}

impl<H, R> ChainWithCont<H, R> {
    fn new(heap: H, parent_cont: Cont<H, R>) -> Self {
        ChainWithCont {
            chain: SlotChain::new(heap),
            parent_cont: std::cell::UnsafeCell::new(Some(parent_cont)),
        }
    }

    fn take_parent_cont(&self) -> Cont<H, R> {
        unsafe { (*self.parent_cont.get()).take().expect("parent_cont already taken") }
    }
}

/// Process a node: init, first_child, dispatch + DFS.
pub fn walk<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    node: N,
    cont: Cont<H, R>,
    view: &PoolExecView,
) where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let heap = fold.init(&node);

    match graph.first_child(&node) {
        None => {
            // Leaf
            let result = fold.finalize(&heap);
            apply_cont(fold, graph, cont, result, view);
        }
        Some((first_child, cursor)) => {
            // Interior: create chain (self-driving fold) with parent cont
            let cwc = Arc::new(ChainWithCont::new(heap, cont));

            // Slot for child 0
            let slot0 = cwc.chain.append_slot();
            let cont0 = Cont::Deliver { slot: slot0, chain: cwc.clone() };

            // Dispatch children 1+ from cursor (may fork to workers)
            dispatch_siblings(fold, graph, cursor, cwc.clone(), 1, view);

            // DFS into child 0 (fused hylomorphism)
            walk(fold, graph, first_child, cont0, view);

            // If cursor had no more children (just child 0), set_total was
            // called inside dispatch_siblings. The walk above delivered to
            // slot0, which may have triggered finalize+apply_cont already.
        }
    }
}

/// Pull children from cursor, create slots, fork via join.
fn dispatch_siblings<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    cursor: ChildCursor<N>,
    cwc: Arc<ChainWithCont<H, R>>,
    index: u32,
    view: &PoolExecView,
) where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    match cursor.next() {
        None => {
            // Cursor exhausted. Set total children count.
            let ready = cwc.chain.set_total(index);
            if ready {
                // All children already delivered before set_total.
                // Drive the chain forward to finalize.
                if let Some(result) = cwc.chain.try_advance::<N>(fold.0) {
                    let parent_cont = cwc.take_parent_cont();
                    apply_cont(fold, graph, parent_cont, result, view);
                }
            }
        }
        Some((child, next_cursor)) => {
            // Create slot for this child
            let slot = cwc.chain.append_slot();
            let child_cont = Cont::Deliver { slot, chain: cwc.clone() };

            // FORK via join: left = walk(child), right = dispatch more
            let cwc2 = cwc.clone();
            view.join(
                || {
                    walk(fold, graph, child, child_cont, view);
                },
                || {
                    dispatch_siblings(fold, graph, next_cursor, cwc2, index + 1, view);
                },
            );
        }
    }
}

/// Deliver a result to a continuation.
fn apply_cont<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    cont: Cont<H, R>,
    result: R,
    view: &PoolExecView,
) where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    match cont {
        Cont::Root(cell) => {
            unsafe { *cell.get() = Some(result); }
        }
        Cont::Deliver { slot, chain } => {
            if let Some(final_result) = chain.chain.deliver::<N>(slot, result, fold.0) {
                // Chain completed. Deliver to parent.
                let parent_cont = chain.take_parent_cont();
                apply_cont(fold, graph, parent_cont, final_result, view);
            }
        }
    }
}

/// Entry point for the executor.
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
    let root_cont = Cont::Root(std::cell::UnsafeCell::new(None));

    walk(&sf, &sg, root.clone(), root_cont, view);

    // walk is synchronous for the root path (join blocks at each level).
    // By the time walk returns, the root result should be written.
    // ... unless workers are still processing and the cascade hasn't
    // reached root yet. In that case we need to help.

    // Actually: walk calls dispatch_siblings which calls view.join().
    // join() blocks until both closures complete. So the main thread
    // blocks at each join until all subtrees are done. By the time
    // the outermost walk returns, everything is done.
    //
    // BUT: the chain's finalize + apply_cont happen inside deliver(),
    // which runs on whichever thread delivers the last result. That
    // might be a worker. The main thread's walk/join chain has returned,
    // but the worker's apply_cont hasn't cascaded to root yet.
    //
    // Hmm, actually: join blocks until BOTH closures complete. The left
    // closure is walk(child_0, cont0). cont0 delivers to the chain.
    // The right closure is dispatch_siblings. Both complete. But the
    // chain's finalize hasn't run yet (it runs when the LAST delivery
    // arrives). Which delivery is last? It could be from child_0 or
    // from a dispatched sibling. The thread running that delivery does
    // the finalize.
    //
    // The join() at the PARENT level blocks until walk(child_0) and
    // dispatch_siblings both return. But walk(child_0) returns after
    // delivering to slot_0 — NOT after the parent finalizes. The
    // parent's finalize runs when all slots are filled, which happens
    // during some apply_cont call on some thread.
    //
    // So: the main thread's stack of join()s does NOT wait for finalize.
    // It waits for all DFS + dispatch to complete, but not for the
    // cascade. The cascade runs asynchronously via apply_cont.
    //
    // This means: after the outermost walk returns, the root's result
    // might not be ready yet. We need to help until it is.

    // Help until root result is ready
    let root_result_ptr = &root_cont;
    loop {
        // Check if result is written
        // ... but root_cont was moved into walk. We can't access it.
        // This is a problem.
        break;
    }

    // DESIGN ISSUE: root_cont was moved into walk(). We can't read it
    // after walk returns. We need a shared location for the root result.
    //
    // Fix: use Arc<UnsafeCell<Option<R>>> for the root result.
    // walk writes to it, run_fold reads from it.
    todo!("root result extraction — need shared result location")
}
