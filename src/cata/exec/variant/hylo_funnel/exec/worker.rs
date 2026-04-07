//! Worker infrastructure: per-worker context, worker loop, job bridge.

use std::cell::Cell;
use std::sync::atomic::Ordering;
use crate::ops::{FoldOps, TreeOps};
use super::super::cont::FunnelTask;
use super::view::FoldView;
use super::super::walk::{WalkCtx, execute_task};
use super::policy::FunnelPolicy;
use super::queue::{WorkStealing, TaskOps};
use super::wake::WakeStrategy;

// ── WorkerCtx (per-worker: shared ctx + queue handle + wake state) ──

pub(crate) struct WorkerCtx<'a, N: Send + 'static, H: 'static, R: Send + 'static, F, G, P: FunnelPolicy> {
    pub(crate) ctx: &'a WalkCtx<F, G, H, R, P>,
    pub(crate) handle: <P::Queue as WorkStealing>::Handle<'a, N, H, R>,
    pub(crate) wake_state: Cell<<P::Wake as WakeStrategy>::State>,
}

// SAFETY: WorkerCtx is only accessed by one thread at a time.
// Cell is !Sync but WorkerCtx is per-thread — never shared.
unsafe impl<N: Send, H, R: Send, F, G, P: FunnelPolicy> Sync for WorkerCtx<'_, N, H, R, F, G, P> {}

impl<N: Clone + Send + 'static, H: 'static, R: Send + 'static, F: FoldOps<N, H, R> + 'static, G: TreeOps<N> + 'static, P: FunnelPolicy>
    WorkerCtx<'_, N, H, R, F, G, P>
{
    pub(crate) fn view(&self) -> &FoldView { unsafe { self.ctx.view_ref() } }

    pub(crate) fn push_task(&self, task: FunnelTask<N, H, R>) {
        if let Some(overflow) = self.handle.push(task) {
            execute_task(self, overflow);
            return;
        }
        let mut state = self.wake_state.get();
        if P::Wake::should_notify(&mut state, self.view().idle_count.load(Ordering::Relaxed)) {
            self.view().notify_idle();
        }
        self.wake_state.set(state);
    }

    pub(crate) fn reset_wake(&self) {
        let mut state = self.wake_state.get();
        P::Wake::reset(&mut state);
        self.wake_state.set(state);
    }
}

// ── FoldState (bridges Job → typed worker code) ──────

pub(crate) struct FoldState<'a, N: Send + 'static, H: 'static, R: Send + 'static, F, G, P: FunnelPolicy> {
    pub(crate) ctx: &'a WalkCtx<F, G, H, R, P>,
    pub(crate) store: *const <P::Queue as WorkStealing>::Store<N, H, R>,
}

unsafe impl<N: Send, H, R: Send, F, G, P: FunnelPolicy> Send for FoldState<'_, N, H, R, F, G, P> {}
unsafe impl<N: Send, H, R: Send, F, G, P: FunnelPolicy> Sync for FoldState<'_, N, H, R, F, G, P> {}

/// Monomorphized worker entry point. The pool calls this through the Job.
pub(crate) unsafe fn worker_entry<N, H, R, F, G, P: FunnelPolicy>(data: *const (), thread_idx: usize)
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let state = unsafe { &*(data as *const FoldState<N, H, R, F, G, P>) };
    let view = unsafe { state.ctx.view_ref() };
    let store = unsafe { &*state.store };
    worker_loop::<N, H, R, F, G, P>(state.ctx, view, store, thread_idx);
}

// ── Worker loop ──────────────────────────────────────

fn worker_loop<N, H, R, F, G, P: FunnelPolicy>(
    ctx: &WalkCtx<F, G, H, R, P>,
    view: &FoldView,
    store: &<P::Queue as WorkStealing>::Store<N, H, R>,
    my_idx: usize,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let handle = P::Queue::handle(store, my_idx);
    let wake_state = Cell::new(P::Wake::init_state(&Default::default()));
    let wctx = WorkerCtx::<N, H, R, F, G, P> { ctx, handle, wake_state };
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
