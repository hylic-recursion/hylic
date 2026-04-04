//! Fork-join mechanism: structured parallelism with stack-scoped ownership race.
//!
//! - [`SyncRef`]: `Send+Sync` wrapper for fork-join scope boundaries.
//! - [`fork_join_map`]: recursive binary-split parallel map over slices.
//!
//! The `join()` method lives on [`PoolExecView`](super::pool::PoolExecView)
//! because it requires direct access to the pool's StealQueue and TaskSlot
//! machinery. `fork_join_map` is a utility built on top of `join`.

use super::pool::PoolExecView;

// ── SyncRef ──────────────────────────────────────────

pub struct SyncRef<'a, T: ?Sized>(pub &'a T);
unsafe impl<T: ?Sized> Sync for SyncRef<'_, T> {}
unsafe impl<T: ?Sized> Send for SyncRef<'_, T> {}
impl<T: ?Sized> std::ops::Deref for SyncRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.0 }
}

// ── fork_join_map ────────────────────────────────────

pub fn fork_join_map<T, R: Send, F: Fn(&T) -> R + Send + Sync>(
    view: &PoolExecView,
    items: &[T],
    f: &F,
    depth: usize,
    max_depth: usize,
) -> Vec<R> {
    let items = SyncRef(items);
    fork_join_map_inner(view, &items, f, depth, max_depth)
}

fn fork_join_map_inner<T, R: Send, F: Fn(&T) -> R + Send + Sync>(
    view: &PoolExecView,
    items: &SyncRef<'_, [T]>,
    f: &F,
    depth: usize,
    max_depth: usize,
) -> Vec<R> {
    if items.len() <= 1 || depth >= max_depth {
        return items.iter().map(|x| f(x)).collect();
    }
    let mid = items.len() / 2;
    let left = SyncRef(&items[..mid]);
    let right = SyncRef(&items[mid..]);
    let (l, r) = view.join(
        || fork_join_map_inner(view, &left, f, depth + 1, max_depth),
        || fork_join_map_inner(view, &right, f, depth + 1, max_depth),
    );
    let mut result = l;
    result.extend(r);
    result
}
