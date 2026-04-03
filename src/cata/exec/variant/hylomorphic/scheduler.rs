//! Scheduler: DFS traversal with tree-navigating work stealing.
//!
//! The DFS thread processes children depth-first. When a worker needs
//! work, it navigates the tree from its last position: walk up to the
//! parent, check right siblings, steal a pending one. O(depth) per
//! steal in theory, O(1) in practice (pending siblings are near).
//!
//! No StealQueue. The arena IS the work structure. Workers navigate it.

use std::cell::Cell;
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

    arena.get(root_id).try_activate();
    process_node(&sf, &sg, root_id, &arena);

    // Help until root is Ready
    let cursor = Cell::new(root_id);
    while arena.get(root_id).state.load(Ordering::Acquire) != READY {
        if let Some(node_id) = find_work_near(&arena, cursor.get()) {
            if arena.get(node_id).try_activate() {
                process_node(&sf, &sg, node_id, &arena);
                cursor.set(node_id);
                continue;
            }
        }
        // No tree work found — help the pool (ParEager Phase 2 etc.)
        if !view.help_once() {
            std::thread::yield_now();
        }
    }

    arena.get(root_id).take_result()
}

fn process_node<N, H, R>(
    fold: &SyncRef<'_, impl FoldOps<N, H, R>>,
    graph: &SyncRef<'_, impl TreeOps<N>>,
    node_id: NodeId,
    arena: &Arena<N, H, R>,
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
        notify_parent(fold, arena, node_id);
        return;
    }

    // DFS into first child
    let first = entry.child_at(0);
    if arena.get(first).try_activate() {
        process_node(fold, graph, first, arena);
    }

    // Process remaining children not yet stolen
    for i in 1..child_count {
        let child_id = entry.child_at(i);
        if arena.get(child_id).try_activate() {
            process_node(fold, graph, child_id, arena);
        }
    }
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

/// Find pending work by navigating from the cursor position.
/// Walk up to parent, check right siblings. O(depth) worst case.
fn find_work_near<N, H, R>(
    arena: &Arena<N, H, R>,
    start: NodeId,
) -> Option<NodeId>
where
    N: Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let mut cursor = start;

    loop {
        let entry = arena.get(cursor);

        // Check: does this node have pending children?
        if entry.children_known.load(Ordering::Acquire) {
            let total = entry.children_total.load(Ordering::Acquire);
            for i in 0..total {
                let child_id = entry.child_at(i);
                if arena.get(child_id).state.load(Ordering::Acquire) == PENDING {
                    return Some(child_id);
                }
            }
        }

        // No pending children here. Walk up.
        match entry.parent {
            Some(parent_id) => cursor = parent_id,
            None => break, // reached root
        }
    }

    // Fallback: scan the arena linearly (rare — only when the tree
    // navigation missed something due to concurrent activation).
    let len = arena.len();
    for i in 0..len {
        let id = NodeId(i);
        if arena.get(id).state.load(Ordering::Acquire) == PENDING {
            return Some(id);
        }
    }

    None
}
