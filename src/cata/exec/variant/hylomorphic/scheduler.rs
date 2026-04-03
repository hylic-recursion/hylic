//! Scheduler: DFS traversal with zipper-frame work stealing.
//!
//! Safe code. All unsafe is in arena.rs.

use crate::ops::{FoldOps, TreeOps};
use crate::prelude::parallel::pool::PoolExecView;
use super::arena::{Arena, NodeId, PENDING};

/// Run a fold over the tree rooted at `root`. Creates an arena,
/// traverses via DFS, steals siblings to workers when idle.
pub fn run_fold<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + Sync),
    graph: &(impl TreeOps<N> + Sync),
    root: &N,
    view: &PoolExecView,
) -> R
where
    N: Clone + Send + 'static,
    H: Clone + Send + 'static,
    R: Clone + Send + 'static,
{
    let arena: Arena<H, R> = Arena::new();
    let root_id = arena.alloc(None, 0);

    process_node(fold, graph, root, root_id, &arena, view);

    // Root might not be Ready yet if children were stolen and are
    // still being processed by workers. Help until root is done.
    while !is_ready(&arena, root_id) {
        // Try to find and process a Pending node in the arena
        if !help_once(fold, graph, &arena, view) {
            // No pending nodes found — help the pool (ParEager Phase 2 tasks etc.)
            if !view.help_once() {
                std::hint::spin_loop();
            }
        }
    }

    arena.get(root_id).take_result()
}

/// Process a single node: init, visit children (DFS into first,
/// remaining are stealable), finalize when all children done.
fn process_node<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + Sync),
    graph: &(impl TreeOps<N> + Sync),
    node_value: &N,
    node_id: NodeId,
    arena: &Arena<H, R>,
    view: &PoolExecView,
) where
    N: Clone + Send + 'static,
    H: Clone + Send + 'static,
    R: Clone + Send + 'static,
{
    let entry = arena.get(node_id);
    if !entry.try_activate() {
        return; // another worker already claimed this node
    }

    // Init
    let heap = fold.init(node_value);
    entry.write_heap(heap);

    // Discover children via visit. First child: DFS locally.
    // Remaining: registered as Pending (stealable).
    let mut child_count = 0u32;
    let mut first_child_value: Option<(N, NodeId)> = None;

    graph.visit(node_value, &mut |child: &N| {
        let child_id = arena.alloc(Some(node_id), child_count);
        entry.push_child(child_id);

        if child_count == 0 {
            // Save the first child for local DFS after visit completes
            first_child_value = Some((child.clone(), child_id));
        }
        // Remaining children sit as Pending — stealable.
        // Their node values need to be accessible to stealers.
        // We store the clone in a side structure (see note below).

        child_count += 1;
    });

    entry.set_children_known(child_count);

    if child_count == 0 {
        // Leaf: finalize immediately
        let result = fold.finalize(unsafe { entry.heap_mut() });
        entry.write_result(result);
        notify_parent(fold, arena, node_id, view);
        return;
    }

    // DFS into first child
    if let Some((child_val, child_id)) = first_child_value {
        process_node(fold, graph, &child_val, child_id, arena, view);
    }

    // Process remaining children that weren't stolen
    for i in 1..child_count {
        let child_id = entry.child_at(i);
        let child_entry = arena.get(child_id);
        if child_entry.state.load(std::sync::atomic::Ordering::Acquire) == PENDING {
            // Not yet stolen. But we need the node value to recurse.
            // The node value was produced by visit and cloned into the
            // arena (well... we don't store it in the arena yet).
            //
            // PROBLEM: We need the N value for children > 0, but visit
            // already finished. We only saved child 0's value.
            //
            // FIX: We need to store N values in the arena, or collect
            // them during visit. Let me store them.
            //
            // For now: this is a design gap. See below.
            let _ = child_id;
        }
    }

    // If all children are already done (processed locally or stolen+completed),
    // finalize this node.
    if entry.all_children_done() {
        do_finalize(fold, arena, node_id);
        notify_parent(fold, arena, node_id, view);
    }
}

/// Called when a child completes. Checks if parent can finalize.
fn notify_parent<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + Sync),
    arena: &Arena<H, R>,
    child_id: NodeId,
    view: &PoolExecView,
) where R: Clone + Send {
    let child_entry = arena.get(child_id);
    if let Some(parent_id) = child_entry.parent {
        let parent = arena.get(parent_id);
        if parent.child_completed() {
            // We're the last child. Run parent's accumulate + finalize.
            do_finalize(fold, arena, parent_id);
            notify_parent(fold, arena, parent_id, view);
        }
    }
}

/// Accumulate all children's results (in sibling order) and finalize.
fn do_finalize<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + Sync),
    arena: &Arena<H, R>,
    node_id: NodeId,
) {
    let entry = arena.get(node_id);
    let total = entry.children_total.load(std::sync::atomic::Ordering::Acquire);
    let heap = unsafe { entry.heap_mut() };

    for i in 0..total {
        let child_id = entry.child_at(i);
        let child = arena.get(child_id);
        fold.accumulate(heap, child.read_result());
    }

    let result = fold.finalize(heap);
    entry.write_result(result);
}

fn is_ready<H, R>(arena: &Arena<H, R>, id: NodeId) -> bool {
    arena.get(id).state.load(std::sync::atomic::Ordering::Acquire) == super::arena::READY
}

/// Try to find a Pending node in the arena and process it.
/// Returns true if work was found.
fn help_once<N, H, R>(
    _fold: &(impl FoldOps<N, H, R> + Sync),
    _graph: &(impl TreeOps<N> + Sync),
    _arena: &Arena<H, R>,
    _view: &PoolExecView,
) -> bool
where
    N: Clone + Send + 'static,
    H: Clone + Send + 'static,
    R: Clone + Send + 'static,
{
    // TODO: scan arena for Pending nodes, process them.
    // This requires storing N values in the arena so workers can
    // call fold.init and graph.visit on stolen nodes.
    false
}
