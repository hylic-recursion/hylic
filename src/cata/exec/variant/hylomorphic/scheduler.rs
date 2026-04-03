//! Scheduler: fully parallel DFS with continuation-based tree navigation.
//!
//! ALL threads are equal. Each runs: pick a Pending node → DFS into it
//! (init, visit, dive into child 0) → when done, go_right to next
//! sibling or go_up to parent's sibling. Leaves trigger finalization
//! that cascades upward. No "main DFS thread" vs "helper workers."
//!
//! When children are discovered, workers are woken to process siblings.
//! The tree structure IS the work distribution mechanism.

use std::sync::atomic::Ordering;
use crate::ops::{FoldOps, TreeOps};
use crate::prelude::parallel::pool::{PoolExecView, SyncRef};
use super::arena::{Arena, NodeId, PENDING, READY};

pub fn run_fold<N, H, R, F, G>(
    fold: &F,
    graph: &G,
    root: &N,
    view: &PoolExecView,
) -> R
where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
    F: FoldOps<N, H, R>,
    G: TreeOps<N>,
{
    let arena: Arena<N, H, R> = Arena::new();
    let root_id = arena.alloc(root.clone(), None, 0);

    let sf = SyncRef(fold);
    let sg = SyncRef(graph);

    // Activate root and enter the worker loop.
    // The main thread IS a worker — it processes nodes just like pool workers.
    arena.get(root_id).try_activate();
    worker_entry(&sf, &sg, root_id, &arena, view);

    // If root isn't Ready yet (other workers still processing subtrees),
    // keep working on tree nodes + pool tasks until done.
    while arena.get(root_id).state.load(Ordering::Acquire) != READY {
        if !try_continue(&sf, &sg, &arena, view, root_id) {
            if !view.help_once() {
                std::thread::yield_now();
            }
        }
    }

    arena.get(root_id).take_result()
}

/// Process a node and follow continuations: after each subtree completes,
/// go_right to next sibling or cascade finalize upward.
fn worker_entry<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    start: NodeId,
    arena: &Arena<N, H, R>,
    view: &PoolExecView,
) where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    // Process the starting node (full DFS into child 0 chain)
    process_node(fold, graph, start, arena, view);

    // After the DFS returns, follow continuations upward:
    // "I finished a subtree — is there a pending sibling? If so, dive in."
    let mut cursor = start;
    loop {
        let entry = arena.get(cursor);

        // Try go_right: find a pending sibling at the same level
        if let Some(parent_id) = entry.parent {
            let parent = arena.get(parent_id);
            if parent.children_known.load(Ordering::Acquire) {
                let total = parent.children_total.load(Ordering::Acquire);
                let my_sib = entry.sibling_index;

                // Check siblings to the right
                for s in (my_sib + 1)..total {
                    let sib_id = parent.child_at(s);
                    if arena.get(sib_id).try_activate() {
                        // Found a pending sibling. DFS into it.
                        process_node(fold, graph, sib_id, arena, view);
                        cursor = sib_id;
                        // After this DFS returns, loop again to check
                        // for more siblings or go_up.
                        continue;
                    }
                }
            }

            // No pending siblings. Go up one level and try again.
            cursor = parent_id;
            continue;
        }

        // Reached root with no more work to do at any level.
        break;
    }
}

/// Process a single node: init, discover children, DFS into child 0.
/// Children 1+ are left as Pending for other workers.
fn process_node<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    node_id: NodeId,
    arena: &Arena<N, H, R>,
    view: &PoolExecView,
) where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let entry = arena.get(node_id);

    let heap = fold.init(entry.node_value());
    entry.write_heap(heap);

    let mut child_count = 0u32;
    graph.visit(entry.node_value(), &mut |child: &N| {
        let child_id = arena.alloc(child.clone(), Some(node_id), child_count);
        entry.push_child(child_id);
        child_count += 1;
    });
    entry.set_children_known(child_count);

    if child_count == 0 {
        // Leaf: finalize immediately, cascade upward
        let result = fold.finalize(unsafe { entry.heap_mut() });
        entry.write_result(result);
        notify_parent(fold, arena, node_id);
        return;
    }

    // Wake workers for siblings 1+
    if child_count > 1 {
        // Workers will find Pending children via try_continue
        for _ in 1..child_count {
            view.wake_workers();
        }
    }

    // DFS into child 0 (the continuation — fused hylomorphism)
    let first = entry.child_at(0);
    if arena.get(first).try_activate() {
        process_node(fold, graph, first, arena, view);
    }

    // Don't process remaining children here — worker_entry's
    // go_right loop handles them (or other workers pick them up).
}

fn notify_parent<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    arena: &Arena<N, H, R>,
    child_id: NodeId,
) where
    N: Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let child_entry = arena.get(child_id);
    if let Some(parent_id) = child_entry.parent {
        let parent = arena.get(parent_id);
        if parent.child_completed() {
            do_finalize(fold, arena, parent_id);
            notify_parent(fold, arena, parent_id);
        }
    }
}

fn do_finalize<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    arena: &Arena<N, H, R>,
    node_id: NodeId,
) where
    N: Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let entry = arena.get(node_id);
    let total = entry.children_total.load(Ordering::Acquire);
    let heap = unsafe { entry.heap_mut() };

    for i in 0..total {
        let child_id = entry.child_at(i);
        fold.accumulate(heap, arena.get(child_id).read_result());
    }

    let result = fold.finalize(heap);
    entry.write_result(result);
}

/// Workers call this to find work in the arena by tree navigation.
fn try_continue<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    arena: &Arena<N, H, R>,
    view: &PoolExecView,
    hint: NodeId,
) -> bool
where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    // Navigate from hint looking for Pending nodes
    if let Some(node_id) = find_pending_near(arena, hint) {
        if arena.get(node_id).try_activate() {
            worker_entry(fold, graph, node_id, arena, view);
            return true;
        }
    }
    false
}

fn find_pending_near<N, H, R>(arena: &Arena<N, H, R>, start: NodeId) -> Option<NodeId>
where N: Send + 'static, H: 'static, R: Send + 'static,
{
    let mut cursor = start;
    loop {
        let entry = arena.get(cursor);
        if entry.children_known.load(Ordering::Acquire) {
            let total = entry.children_total.load(Ordering::Acquire);
            for i in 0..total {
                let child_id = entry.child_at(i);
                if arena.get(child_id).state.load(Ordering::Acquire) == PENDING {
                    return Some(child_id);
                }
            }
        }
        match entry.parent {
            Some(parent_id) => cursor = parent_id,
            None => break,
        }
    }
    // Fallback: scan (rare)
    let len = arena.len();
    for i in 0..len {
        if arena.get(NodeId(i)).state.load(Ordering::Acquire) == PENDING {
            return Some(NodeId(i));
        }
    }
    None
}
