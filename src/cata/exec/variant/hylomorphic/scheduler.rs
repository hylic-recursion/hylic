//! Scheduler: DFS traversal with arena-based work stealing.
//! Safe code over arena's atomic API.

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

    // Wrap fold/graph in SyncRef for cross-thread sharing.
    // Safe: all workers join before run_fold returns (scoped pool).
    let sf = SyncRef(fold);
    let sg = SyncRef(graph);

    // Activate root and process it
    let root_entry = arena.get(root_id);
    root_entry.try_activate(); // root is always ours

    process_node(&sf, &sg, root_id, &arena, view);

    while arena.get(root_id).state.load(Ordering::Acquire) != READY {
        if !help_once(&sf, &sg, &arena, view) {
            if !view.help_once() {
                std::hint::spin_loop();
            }
        }
    }

    arena.get(root_id).take_result()
}

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
        let result = fold.finalize(unsafe { entry.heap_mut() });
        entry.write_result(result);
        notify_parent(fold, arena, node_id, view);
        return;
    }

    // DFS into first child
    let first = entry.child_at(0);
    if arena.get(first).try_activate() {
        process_node(fold, graph, first, arena, view);
    }

    // Process remaining children not yet stolen
    for i in 1..child_count {
        let child_id = entry.child_at(i);
        if arena.get(child_id).try_activate() {
            process_node(fold, graph, child_id, arena, view);
        }
    }

    // Don't finalize here — notify_parent cascade handles it.
    // The last child to complete (via notify_parent) runs
    // do_finalize + notify_parent on this node.
}

fn notify_parent<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    arena: &Arena<N, H, R>,
    child_id: NodeId,
    view: &PoolExecView,
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
            notify_parent(fold, arena, parent_id, view);
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

fn help_once<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    arena: &Arena<N, H, R>,
    view: &PoolExecView,
) -> bool
where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let len = arena.len();
    for i in 0..len {
        let id = NodeId(i);
        if arena.get(id).state.load(Ordering::Acquire) == PENDING {
            if arena.get(id).try_activate() {
                process_node(fold, graph, id, arena, view);
                return true;
            }
        }
    }
    false
}
