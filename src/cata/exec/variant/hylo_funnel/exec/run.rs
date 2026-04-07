//! Entry points: FoldContext pre-allocation, run_fold, run_fold_with.

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::super::cont::{Cont, RootCell, ChainNode};
use super::view::FoldView;
use super::super::walk::{WalkCtx, walk_cps, execute_task};
use super::worker::{WorkerCtx, FoldState, worker_entry};
use super::policy::{self, FunnelPolicy};
use super::queue::{WorkStealing, TaskOps};
use super::wake::WakeStrategy;
use super::super::pool::{FunnelPool, Job};
use super::super::arena::Arena;
use super::super::cont_arena::ContArena;

const CHAIN_ARENA_CAPACITY: usize = 4096;
const CONT_ARENA_CAPACITY: usize = 8192;

// ── FoldContext (pre-allocatable typed state) ────────

#[allow(private_interfaces)]
pub struct FoldContext<N: Send + 'static, H: 'static, R: Send + 'static, P: FunnelPolicy = policy::Default> {
    pub store: <P::Queue as WorkStealing>::Store<N, H, R>,
    pub chain_arena: Arena<ChainNode<H, R>>,
    pub cont_arena: ContArena<Cont<H, R>>,
}

impl<N: Send + 'static, H: 'static, R: Send + 'static, P: FunnelPolicy> FoldContext<N, H, R, P> {
    pub fn new(queue_spec: &<P::Queue as WorkStealing>::Spec, n_threads: usize) -> Self {
        FoldContext {
            store: P::Queue::create_store(queue_spec, n_threads),
            chain_arena: Arena::new(CHAIN_ARENA_CAPACITY),
            cont_arena: ContArena::new(CONT_ARENA_CAPACITY),
        }
    }

    pub fn reset(&mut self) {
        P::Queue::reset_store(&mut self.store);
        self.chain_arena.reset();
        self.cont_arena.reset();
    }
}

// ── run_fold ─────────────────────────────────────────

pub fn run_fold<N, H, R, F, G, P: FunnelPolicy>(
    fold: &F,
    graph: &G,
    root: &N,
    pool: &FunnelPool,
    queue_spec: &<P::Queue as WorkStealing>::Spec,
    wake_spec: &<P::Wake as WakeStrategy>::Spec,
) -> R
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let mut fctx = FoldContext::<N, H, R, P>::new(queue_spec, pool.n_threads());
    run_fold_with::<N, H, R, F, G, P>(fold, graph, root, pool, wake_spec, &mut fctx)
}

/// Run a fold using pre-allocated typed state. Call fctx.reset() before each fold.
pub fn run_fold_with<N, H, R, F, G, P: FunnelPolicy>(
    fold: &F,
    graph: &G,
    root: &N,
    pool: &FunnelPool,
    wake_spec: &<P::Wake as WakeStrategy>::Spec,
    fctx: &mut FoldContext<N, H, R, P>,
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

    let ctx = WalkCtx::<F, G, H, R, P> {
        fold: fold as *const _,
        graph: graph as *const _,
        view: &view as *const FoldView,
        chain_arena: &fctx.chain_arena as *const _,
        cont_arena: &fctx.cont_arena as *const _,
        _policy: std::marker::PhantomData,
    };

    let state = FoldState::<N, H, R, F, G, P> {
        ctx: &ctx,
        store: &fctx.store as *const _,
    };
    let job = Job {
        call: worker_entry::<N, H, R, F, G, P>,
        data: &state as *const FoldState<N, H, R, F, G, P> as *const (),
    };

    pool.dispatch(&job, || {
        let caller_idx = view.n_workers;
        let handle = P::Queue::handle(&fctx.store, caller_idx);
        let wake_state = Cell::new(P::Wake::init_state(wake_spec));
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

        view.pool_inner.job_ptr.store(std::ptr::null_mut(), Ordering::Release);

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
