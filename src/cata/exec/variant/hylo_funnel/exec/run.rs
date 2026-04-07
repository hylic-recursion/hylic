//! Entry point: run_fold. Creates typed state per fold, dispatches to workers.

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::super::cont::{Cont, RootCell, ChainNode};
use super::super::Spec;
use super::view::FoldView;
use super::super::walk::{WalkCtx, walk_cps, execute_task};
use super::worker::{WorkerCtx, FoldState, worker_entry};
use super::policy::FunnelPolicy;
use super::queue::{WorkStealing, TaskOps};
use super::wake::WakeStrategy;
use super::super::pool::{PoolState, Job, dispatch};
use super::super::arena::Arena;
use super::super::cont_arena::ContArena;

pub(crate) fn run_fold<N, H, R, F, G, P: FunnelPolicy>(
    fold: &F, graph: &G, root: &N,
    pool_state: &PoolState, spec: &Spec<P>,
) -> R
where
    F: FoldOps<N, H, R> + 'static, G: TreeOps<N> + 'static,
    N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static,
{
    let store = P::Queue::create_store(&spec.queue, pool_state.n_threads);
    let chain_arena = Arena::<ChainNode<H, R>>::new(spec.chain_arena_capacity);
    let cont_arena = ContArena::<Cont<H, R>>::new(spec.cont_arena_capacity);
    let root_cell = Arc::new(RootCell::new());

    let view = FoldView {
        pool_state,
        fold_done: AtomicBool::new(false),
        idle_count: AtomicU32::new(0),
        workers_active: AtomicU32::new(0),
        n_workers: pool_state.n_threads,
    };

    let ctx = WalkCtx {
        fold,
        graph,
        view: &view,
        chain_arena: &chain_arena,
        cont_arena: &cont_arena,
        _policy: std::marker::PhantomData,
    };

    let state = FoldState::<N, H, R, F, G, P> {
        ctx: &ctx,
        store: &store,
    };
    // The ONE unsafe boundary: erase typed FoldState to *const () for the Job.
    let job = Job {
        call: worker_entry::<N, H, R, F, G, P>,
        data: &state as *const FoldState<N, H, R, F, G, P> as *const (),
    };

    dispatch(pool_state, &job, || {
        let caller_idx = view.n_workers;
        let handle = P::Queue::handle(&store, caller_idx);
        let wake_state = Cell::new(P::Wake::init_state(&spec.wake));
        let wctx = WorkerCtx::<N, H, R, F, G, P> { ctx: &ctx, handle, wake_state };

        walk_cps(&wctx, root.clone(), Cont::Root(root_cell.clone()));

        let mut spins = 0u64;
        while !root_cell.is_done() {
            if let Some(task) = wctx.handle.try_acquire() {
                execute_task(&wctx, task);
                spins = 0;
            } else {
                spins += 1;
                if spins > 10_000_000 {
                    panic!("run_fold hung: root_done={}", root_cell.is_done());
                }
                std::hint::spin_loop();
            }
        }

        pool_state.job_ptr.store(std::ptr::null_mut(), Ordering::Release);

        let mut barrier_spins = 0u32;
        while view.workers_active.load(Ordering::Acquire) > 0 {
            barrier_spins += 1;
            if barrier_spins > 5_000_000 {
                panic!("workers_active: {} still active after fold complete",
                    view.workers_active.load(Ordering::Relaxed));
            }
            std::hint::spin_loop();
        }

        root_cell.take()
    })
}
