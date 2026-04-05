//! Worker infrastructure: per-worker context, worker loop, job bridge.

use crate::ops::{FoldOps, TreeOps};
use super::cont::FunnelTask;
use super::deque::WorkerDeque;
use super::view::{FoldView, steal_from_others};
use super::walk::{WalkCtx, execute_task};

// ── WorkerCtx (per-worker: shared ctx + own deque + index) ──

pub(super) struct WorkerCtx<'a, N, H, R, F, G> {
    pub(super) ctx: &'a WalkCtx<F, G, H, R>,
    pub(super) deque: &'a WorkerDeque<FunnelTask<N, H, R>>,
    pub(super) deque_idx: usize,
}

impl<N, H, R, F, G> WorkerCtx<'_, N, H, R, F, G> {
    pub(super) fn view(&self) -> &FoldView { unsafe { self.ctx.view_ref() } }

    pub(super) fn push_task(&self, task: FunnelTask<N, H, R>) {
        let pushed = self.deque.push(task);
        assert!(pushed, "deque full");
        self.view().signal_push(self.deque_idx);
    }
}

// ── FoldState (bridges Job → typed worker code) ──────

pub(super) struct FoldState<'a, N, H, R, F, G> {
    pub(super) ctx: &'a WalkCtx<F, G, H, R>,
    pub(super) deques: *const [WorkerDeque<FunnelTask<N, H, R>>],
}

unsafe impl<N: Send, H, R: Send, F, G> Send for FoldState<'_, N, H, R, F, G> {}
unsafe impl<N: Send, H, R: Send, F, G> Sync for FoldState<'_, N, H, R, F, G> {}

/// Monomorphized worker entry point. The pool calls this through the Job.
pub(super) unsafe fn worker_entry<N, H, R, F, G>(data: *const (), thread_idx: usize)
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let state = unsafe { &*(data as *const FoldState<N, H, R, F, G>) };
    let view = unsafe { state.ctx.view_ref() };
    let deques = unsafe { &*state.deques };
    worker_loop(state.ctx, view, deques, thread_idx);
}

// ── Worker loop (inside the View) ────────────────────

fn worker_loop<N, H, R, F, G>(
    ctx: &WalkCtx<F, G, H, R>,
    view: &FoldView,
    deques: &[WorkerDeque<FunnelTask<N, H, R>>],
    my_idx: usize,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let wctx = WorkerCtx { ctx, deque: &deques[my_idx], deque_idx: my_idx };
    loop {
        if let Some(task) = wctx.deque.pop() {
            execute_task(&wctx, task);
            continue;
        }
        if let Some(task) = steal_from_others(deques, my_idx, view) {
            execute_task(&wctx, task);
            continue;
        }
        let event = view.event();
        let token = event.prepare();
        if view.fold_done.load(std::sync::atomic::Ordering::Acquire) { return; }
        if let Some(task) = wctx.deque.pop() {
            execute_task(&wctx, task);
            continue;
        }
        if let Some(task) = steal_from_others(deques, my_idx, view) {
            execute_task(&wctx, task);
            continue;
        }
        view.idle_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        event.wait(token);
        view.idle_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}
