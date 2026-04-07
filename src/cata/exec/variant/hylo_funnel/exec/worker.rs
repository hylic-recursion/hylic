//! Worker infrastructure: per-worker context, worker loop, job bridge.

use std::sync::atomic::Ordering;
use crate::ops::{FoldOps, TreeOps};
use super::super::cont::FunnelTask;
use super::view::FoldView;
use super::super::walk::{WalkCtx, execute_task};
use super::queue::{WorkStealing, TaskOps};

// ── WorkerCtx (per-worker: shared ctx + queue handle) ──

pub(crate) struct WorkerCtx<'a, N: Send + 'static, H: 'static, R: Send + 'static, F, G, W: WorkStealing> {
    pub(crate) ctx: &'a WalkCtx<F, G, H, R>,
    pub(crate) handle: W::Handle<'a, N, H, R>,
}

impl<N: Send + 'static, H: 'static, R: Send + 'static, F, G, W: WorkStealing>
    WorkerCtx<'_, N, H, R, F, G, W>
{
    pub(crate) fn view(&self) -> &FoldView { unsafe { self.ctx.view_ref() } }

    pub(crate) fn push_task(&self, task: FunnelTask<N, H, R>) {
        self.handle.push(task);
        self.view().notify_idle();
    }
}

// ── FoldState (bridges Job → typed worker code) ──────

pub(crate) struct FoldState<'a, N: Send + 'static, H: 'static, R: Send + 'static, F, G, W: WorkStealing> {
    pub(crate) ctx: &'a WalkCtx<F, G, H, R>,
    pub(crate) store: *const W::Store<N, H, R>,
}

unsafe impl<N: Send, H, R: Send, F, G, W: WorkStealing> Send for FoldState<'_, N, H, R, F, G, W> {}
unsafe impl<N: Send, H, R: Send, F, G, W: WorkStealing> Sync for FoldState<'_, N, H, R, F, G, W> {}

/// Monomorphized worker entry point. The pool calls this through the Job.
pub(crate) unsafe fn worker_entry<N, H, R, F, G, W: WorkStealing>(data: *const (), thread_idx: usize)
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let state = unsafe { &*(data as *const FoldState<N, H, R, F, G, W>) };
    let view = unsafe { state.ctx.view_ref() };
    let store = unsafe { &*state.store };
    worker_loop::<N, H, R, F, G, W>(state.ctx, view, store, thread_idx);
}

// ── Worker loop ──────────────────────────────────────

fn worker_loop<N, H, R, F, G, W: WorkStealing>(
    ctx: &WalkCtx<F, G, H, R>,
    view: &FoldView,
    store: &W::Store<N, H, R>,
    my_idx: usize,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let handle = W::handle(store, my_idx);
    let wctx = WorkerCtx::<N, H, R, F, G, W> { ctx, handle };
    loop {
        if let Some(task) = wctx.handle.try_acquire() {
            execute_task(&wctx, task);
            continue;
        }
        let event = view.event();
        let token = event.prepare();
        if view.fold_done.load(Ordering::Acquire) { return; }
        if let Some(task) = wctx.handle.try_acquire() {
            execute_task(&wctx, task);
            continue;
        }
        view.idle_count.fetch_add(1, Ordering::Relaxed);
        event.wait(token);
        view.idle_count.fetch_sub(1, Ordering::Relaxed);
    }
}
