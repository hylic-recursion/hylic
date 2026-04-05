//! Entry points: FoldContext pre-allocation, run_fold, run_fold_with.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::cont::{FunnelTask, Cont, RootCell, ChainNode};
use super::view::{FoldView, steal_from_others};
use super::walk::{WalkCtx, walk_cps, execute_task};
use super::worker::{WorkerCtx, FoldState, worker_entry};
use super::pool::{FunnelPool, Job, DEQUE_CAPACITY};
use super::deque::WorkerDeque;
use super::arena::Arena;
use super::cont_arena::ContArena;
use super::AccumulateMode;

// ── Constants ────────────────────────────────────────

const CHAIN_ARENA_CAPACITY: usize = 4096;
const CONT_ARENA_CAPACITY: usize = 8192;

// ── FoldContext (pre-allocatable typed state) ────────

/// Pre-allocatable typed state for repeated folds. Create once, call
/// reset() between folds. Avoids deque + arena allocation per fold.
#[allow(private_interfaces)]
pub struct FoldContext<N, H, R> {
    pub deques: Vec<WorkerDeque<FunnelTask<N, H, R>>>,
    pub chain_arena: Arena<ChainNode<H, R>>,
    pub cont_arena: ContArena<Cont<H, R>>,
}

impl<N, H, R> FoldContext<N, H, R> {
    pub fn new(n_threads: usize) -> Self {
        FoldContext {
            deques: (0..n_threads + 1)
                .map(|_| WorkerDeque::new(DEQUE_CAPACITY))
                .collect(),
            chain_arena: Arena::new(CHAIN_ARENA_CAPACITY),
            cont_arena: ContArena::new(CONT_ARENA_CAPACITY),
        }
    }

    pub fn reset(&mut self) {
        // Deques should be empty after a fold (all tasks executed).
        // Arenas: drop old data, reset bump pointers.
        self.chain_arena.reset();
        self.cont_arena.reset();
    }
}

// ── run_fold ─────────────────────────────────────────

pub fn run_fold<N, H, R, F, G>(
    fold: &F,
    graph: &G,
    root: &N,
    pool: &FunnelPool,
    accumulate: AccumulateMode,
) -> R
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    let mut fctx = FoldContext::<N, H, R>::new(pool.n_threads());
    run_fold_with(fold, graph, root, pool, accumulate, &mut fctx)
}

/// Run a fold using pre-allocated typed state. Call fctx.reset() before each fold.
pub fn run_fold_with<N, H, R, F, G>(
    fold: &F,
    graph: &G,
    root: &N,
    pool: &FunnelPool,
    accumulate: AccumulateMode,
    fctx: &mut FoldContext<N, H, R>,
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
        work_available: AtomicU64::new(0),
        n_workers: pool.n_threads(),
    };
    let deques = fctx.deques.as_slice();

    let ctx = WalkCtx {
        fold: fold as *const _,
        graph: graph as *const _,
        view: &view as *const FoldView,
        chain_arena: &fctx.chain_arena as *const _,
        cont_arena: &fctx.cont_arena as *const _,
        accumulate,
    };

    let state = FoldState {
        ctx: &ctx,
        deques: deques as *const [WorkerDeque<FunnelTask<N, H, R>>],
    };
    let job = Job {
        call: worker_entry::<N, H, R, F, G>,
        data: &state as *const FoldState<N, H, R, F, G> as *const (),
    };

    pool.dispatch(&job, || {
        let caller_idx = view.n_workers;
        let wctx = WorkerCtx { ctx: &ctx, deque: &deques[caller_idx], deque_idx: caller_idx };

        walk_cps(&wctx, root.clone(), Cont::Root(root_cell.clone()));

        let mut spins = 0u64;
        while !root_cell.is_done() {
            if let Some(task) = wctx.deque.pop() {
                execute_task(&wctx, task);
                spins = 0;
            } else if let Some(task) = steal_from_others(deques, caller_idx, &view) {
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

        // Clear job_ptr BEFORE the latch. This prevents new workers from
        // entering the job after the latch passes. Workers that already
        // incremented in_job will either call the job (stack still alive)
        // or see null and skip. The latch waits for all of them.
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
