//! Entry points: FoldContext pre-allocation, run_fold, run_fold_with.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::super::cont::{Cont, RootCell, ChainNode};
use super::view::FoldView;
use super::super::walk::{WalkCtx, walk_cps, execute_task};
use super::worker::{WorkerCtx, FoldState, worker_entry};
use super::queue::{WorkStealing, TaskOps, per_worker::PerWorker};
use super::super::pool::{FunnelPool, Job};
use super::super::arena::Arena;
use super::super::cont_arena::ContArena;
use super::super::AccumulateMode;

const CHAIN_ARENA_CAPACITY: usize = 4096;
const CONT_ARENA_CAPACITY: usize = 8192;

// ── FoldContext (pre-allocatable typed state) ────────

#[allow(private_interfaces)]
pub struct FoldContext<N: Send + 'static, H: 'static, R: Send + 'static, W: WorkStealing = PerWorker> {
    pub store: W::Store<N, H, R>,
    pub chain_arena: Arena<ChainNode<H, R>>,
    pub cont_arena: ContArena<Cont<H, R>>,
}

impl<N: Send + 'static, H: 'static, R: Send + 'static, W: WorkStealing> FoldContext<N, H, R, W> {
    pub fn new(queue_spec: &W::Spec, n_threads: usize) -> Self {
        FoldContext {
            store: W::create_store(queue_spec, n_threads),
            chain_arena: Arena::new(CHAIN_ARENA_CAPACITY),
            cont_arena: ContArena::new(CONT_ARENA_CAPACITY),
        }
    }

    pub fn reset(&mut self) {
        W::reset_store(&mut self.store);
        self.chain_arena.reset();
        self.cont_arena.reset();
    }
}

// ── run_fold ─────────────────────────────────────────

pub fn run_fold<N, H, R, F, G, W: WorkStealing>(
    fold: &F,
    graph: &G,
    root: &N,
    pool: &FunnelPool,
    accumulate: AccumulateMode,
    queue_spec: &W::Spec,
) -> R
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let mut fctx = FoldContext::<N, H, R, W>::new(queue_spec, pool.n_threads());
    run_fold_with::<N, H, R, F, G, W>(fold, graph, root, pool, accumulate, &mut fctx)
}

/// Run a fold using pre-allocated typed state. Call fctx.reset() before each fold.
pub fn run_fold_with<N, H, R, F, G, W: WorkStealing>(
    fold: &F,
    graph: &G,
    root: &N,
    pool: &FunnelPool,
    accumulate: AccumulateMode,
    fctx: &mut FoldContext<N, H, R, W>,
) -> R
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let root_cell = Arc::new(RootCell::new());
    let view = FoldView {
        pool_inner: pool.inner().clone(),
        fold_done: AtomicBool::new(false),
        idle_count: AtomicU32::new(0),
        n_workers: pool.n_threads(),
    };

    let ctx = WalkCtx {
        fold: fold as *const _,
        graph: graph as *const _,
        view: &view as *const FoldView,
        chain_arena: &fctx.chain_arena as *const _,
        cont_arena: &fctx.cont_arena as *const _,
        accumulate,
    };

    let state = FoldState::<N, H, R, F, G, W> {
        ctx: &ctx,
        store: &fctx.store as *const W::Store<N, H, R>,
    };
    let job = Job {
        call: worker_entry::<N, H, R, F, G, W>,
        data: &state as *const FoldState<N, H, R, F, G, W> as *const (),
    };

    pool.dispatch(&job, || {
        let caller_idx = view.n_workers;
        let handle = W::handle(&fctx.store, caller_idx);
        let wctx = WorkerCtx::<N, H, R, F, G, W> { ctx: &ctx, handle };

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

        // Clear job_ptr before latch — prevents new workers from entering.
        view.pool_inner.job_ptr.store(std::ptr::null_mut(), Ordering::Release);

        // Latch: wait for all pool threads to exit the job call.
        let mut latch_spins = 0u32;
        while view.pool_inner.in_job.load(Ordering::Acquire) > 0 {
            latch_spins += 1;
            if latch_spins > 5_000_000 {
                panic!("latch: {} threads still in job after fold complete",
                    view.pool_inner.in_job.load(Ordering::Relaxed));
            }
            std::hint::spin_loop();
        }

        root_cell.take()
    })
}
