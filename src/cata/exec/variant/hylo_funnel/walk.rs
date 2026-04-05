//! CPS walk for hylo-funnel: defunctionalized tasks, per-worker deques,
//! packed-ticket streaming sweep, no rakers.
//!
//! Tasks are data (FunnelTask::Walk only). Workers pattern-match.
//! Each delivery callback tries the sweep permission inline.
//! The packed ticket (Relaxed AtomicU64) determines who finalizes.
//! Per-slot filled.load(Acquire) provides data visibility.
//! fold_done set inside fire_cont(Cont::Root) — CPS completion signal.
//!
//! All per-fold state is stack-local to run_fold. WalkCtx is shared
//! by &reference (immutable, created once). The Job struct bridges
//! the pool's type-erased dispatch to the typed worker code.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::fold_chain::{FoldChain, SlotRef};
use super::arena::{Arena, ArenaIdx};
use super::cont_arena::{ContArena, ContIdx};
use super::deque::WorkerDeque;
use super::pool::{FunnelPool, FoldView, Job, steal_from_others, DEQUE_CAPACITY};

// ── Defunctionalized task ────────────────────────────

#[allow(private_interfaces)]
pub(super) enum FunnelTask<N, H, R> {
    Walk { child: N, cont: Cont<H, R> },
}

unsafe impl<N: Send, H, R: Send> Send for FunnelTask<N, H, R> {}

// ── Shared immutable context (created once, passed by &ref) ──

struct WalkCtx<F, G, H, R> {
    fold: *const F,
    graph: *const G,
    view: *const FoldView,
    chain_arena: *const Arena<ChainNode<H, R>>,
    cont_arena: *const ContArena<Cont<H, R>>,
}

unsafe impl<F, G, H, R> Send for WalkCtx<F, G, H, R> {}
unsafe impl<F, G, H, R> Sync for WalkCtx<F, G, H, R> {}

impl<F, G, H, R> WalkCtx<F, G, H, R> {
    unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }
    unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
    unsafe fn view_ref(&self) -> &FoldView { unsafe { &*self.view } }
    unsafe fn chain_arena(&self) -> &Arena<ChainNode<H, R>> { unsafe { &*self.chain_arena } }
    unsafe fn cont_arena(&self) -> &ContArena<Cont<H, R>> { unsafe { &*self.cont_arena } }
}

// ── FoldState (bridges Job → typed worker code) ──────

struct FoldState<'a, N, H, R, F, G> {
    ctx: &'a WalkCtx<F, G, H, R>,
    deques: *const [WorkerDeque<FunnelTask<N, H, R>>],
}

unsafe impl<N: Send, H, R: Send, F, G> Send for FoldState<'_, N, H, R, F, G> {}
unsafe impl<N: Send, H, R: Send, F, G> Sync for FoldState<'_, N, H, R, F, G> {}

/// Monomorphized worker entry point. The pool calls this through the Job.
unsafe fn worker_entry<N, H, R, F, G>(data: *const (), thread_idx: usize)
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

// ── Defunctionalized continuation ─────────────────────

enum Cont<H, R> {
    Root(Arc<RootCell<R>>),
    Direct { heap: H, parent_idx: ContIdx },
    Slot { node: ArenaIdx, slot: SlotRef },
}

unsafe impl<H, R: Send> Send for Cont<H, R> {}

// ── ChainNode (multi-child only, arena-allocated) ────

struct ChainNode<H, R> {
    chain: FoldChain<H, R>,
    parent_cont: UnsafeCell<Option<Cont<H, R>>>,
}

unsafe impl<H, R: Send> Send for ChainNode<H, R> {}
unsafe impl<H, R: Send> Sync for ChainNode<H, R> {}

impl<H, R> ChainNode<H, R> {
    fn new(heap: H, cont: Cont<H, R>) -> Self {
        ChainNode { chain: FoldChain::new(heap), parent_cont: UnsafeCell::new(Some(cont)) }
    }
    fn take_parent_cont(&self) -> Cont<H, R> {
        unsafe { (*self.parent_cont.get()).take().expect("parent cont already taken") }
    }
}

// ── Root result cell ──────────────────────────────────

struct RootCell<R> {
    result: UnsafeCell<Option<R>>,
    done: AtomicBool,
}

unsafe impl<R: Send> Send for RootCell<R> {}
unsafe impl<R: Send> Sync for RootCell<R> {}

impl<R> RootCell<R> {
    fn new() -> Self { RootCell { result: UnsafeCell::new(None), done: AtomicBool::new(false) } }
    fn set(&self, r: R) {
        unsafe { *self.result.get() = Some(r); }
        self.done.store(true, Ordering::Release);
    }
    fn is_done(&self) -> bool { self.done.load(Ordering::Acquire) }
    fn take(&self) -> R { unsafe { (*self.result.get()).take().expect("root result not set") } }
}

// ── fire_cont (trampolined) ──────────────────────────

fn fire_cont<N, H, R, F, G>(
    ctx: &WalkCtx<F, G, H, R>,
    mut cont: Cont<H, R>,
    mut result: R,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    loop {
        match cont {
            Cont::Root(cell) => {
                cell.set(result);
                let view = unsafe { ctx.view_ref() };
                view.fold_done.store(true, Ordering::Release);
                view.event().notify_all();
                return;
            }
            Cont::Direct { mut heap, parent_idx } => {
                let fold = unsafe { ctx.fold_ref() };
                fold.accumulate(&mut heap, &result);
                result = fold.finalize(&heap);
                cont = unsafe { ctx.cont_arena().take(parent_idx) };
            }
            Cont::Slot { node: node_idx, slot } => {
                let arena = unsafe { ctx.chain_arena() };
                let node = unsafe { arena.get(node_idx) };
                let fold = unsafe { ctx.fold_ref() };
                match node.chain.deliver_and_sweep(slot, result, fold) {
                    Some(finalized) => {
                        cont = node.take_parent_cont();
                        result = finalized;
                    }
                    None => return,
                }
            }
        }
    }
}

// ── CPS walk ─────────────────────────────────────────

fn walk_cps<N, H, R, F, G>(
    ctx: &WalkCtx<F, G, H, R>,
    node: N,
    cont: Cont<H, R>,
    deque: &WorkerDeque<FunnelTask<N, H, R>>,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let fold = unsafe { ctx.fold_ref() };
    let graph = unsafe { ctx.graph_ref() };
    let chain_arena = unsafe { ctx.chain_arena() };
    let cont_arena = unsafe { ctx.cont_arena() };
    let view = unsafe { ctx.view_ref() };
    let heap = fold.init(&node);

    let mut child_count = 0u32;
    let mut first_child: Option<N> = None;
    let mut chain_idx: Option<ArenaIdx> = None;
    let mut heap_opt = Some(heap);
    let mut cont_opt = Some(cont);

    graph.visit(&node, &mut |child: &N| {
        child_count += 1;
        if child_count == 1 {
            first_child = Some(child.clone());
        } else {
            if child_count == 2 {
                let cn = ChainNode::new(heap_opt.take().unwrap(), cont_opt.take().unwrap());
                let idx = chain_arena.alloc(cn);
                let node_ref = unsafe { chain_arena.get(idx) };
                node_ref.chain.append_slot();
                chain_idx = Some(idx);
            }
            let idx = chain_idx.unwrap();
            let node_ref = unsafe { chain_arena.get(idx) };
            let slot = node_ref.chain.append_slot();
            let pushed = deque.push(FunnelTask::Walk {
                child: child.clone(),
                cont: Cont::Slot { node: idx, slot },
            });
            assert!(pushed, "deque full");
            view.notify_one();
        }
    });

    match child_count {
        0 => {
            let heap = heap_opt.take().unwrap();
            let cont = cont_opt.take().unwrap();
            let result = fold.finalize(&heap);
            fire_cont::<N, H, R, F, G>(ctx, cont, result);
        }
        1 => {
            let child = first_child.unwrap();
            let heap = heap_opt.take().unwrap();
            let parent_cont = cont_opt.take().unwrap();
            let parent_idx = cont_arena.alloc(parent_cont);
            walk_cps::<N, H, R, F, G>(ctx, child, Cont::Direct { heap, parent_idx }, deque);
        }
        _ => {
            let idx = chain_idx.unwrap();
            let cn = unsafe { chain_arena.get(idx) };
            let fold = unsafe { ctx.fold_ref() };
            if let Some(finalized) = cn.chain.set_total_and_sweep(fold) {
                let parent = cn.take_parent_cont();
                fire_cont::<N, H, R, F, G>(ctx, parent, finalized);
                return;
            }
            let child = first_child.unwrap();
            walk_cps::<N, H, R, F, G>(ctx, child, Cont::Slot {
                node: idx, slot: SlotRef(0),
            }, deque);
        }
    }
}

// ── Execute task ─────────────────────────────────────

fn execute_task<N, H, R, F, G>(
    ctx: &WalkCtx<F, G, H, R>,
    task: FunnelTask<N, H, R>,
    deque: &WorkerDeque<FunnelTask<N, H, R>>,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    match task {
        FunnelTask::Walk { child, cont } => walk_cps(ctx, child, cont, deque),
    }
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
    view.active_in_view.fetch_add(1, Ordering::Relaxed);
    let my_deque = &deques[my_idx];
    loop {
        if let Some(task) = my_deque.pop() {
            execute_task(ctx, task, my_deque);
            continue;
        }
        if let Some(task) = steal_from_others(deques, my_idx) {
            execute_task(ctx, task, my_deque);
            continue;
        }
        let event = view.event();
        let token = event.prepare();
        if view.fold_done.load(Ordering::Acquire) {
            view.active_in_view.fetch_sub(1, Ordering::Release);
            return;
        }
        if let Some(task) = my_deque.pop() {
            execute_task(ctx, task, my_deque);
            continue;
        }
        if let Some(task) = steal_from_others(deques, my_idx) {
            execute_task(ctx, task, my_deque);
            continue;
        }
        view.idle_count.fetch_add(1, Ordering::Relaxed);
        event.wait(token);
        view.idle_count.fetch_sub(1, Ordering::Relaxed);
    }
}

// ── Entry point ───────────────────────────────────────

const CHAIN_ARENA_CAPACITY: usize = 4096;
const CONT_ARENA_CAPACITY: usize = 8192;

pub fn run_fold<N, H, R, F, G>(
    fold: &F,
    graph: &G,
    root: &N,
    pool: &FunnelPool,
) -> R
where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    // All per-fold state: stack-local.
    let chain_arena = Arena::<ChainNode<H, R>>::new(CHAIN_ARENA_CAPACITY);
    let cont_arena = ContArena::<Cont<H, R>>::new(CONT_ARENA_CAPACITY);
    let root_cell = Arc::new(RootCell::new());
    let deques: Vec<WorkerDeque<FunnelTask<N, H, R>>> =
        (0..pool.n_threads() + 1)
            .map(|_| WorkerDeque::new(DEQUE_CAPACITY))
            .collect();
    let view = FoldView {
        pool_inner: pool.inner().clone(),
        fold_done: AtomicBool::new(false),
        idle_count: AtomicU32::new(0),
        active_in_view: AtomicU32::new(0),
        n_workers: pool.n_threads(),
    };

    // Immutable shared context — created once, passed by &reference.
    let ctx = WalkCtx {
        fold: fold as *const _,
        graph: graph as *const _,
        view: &view as *const FoldView,
        chain_arena: &chain_arena as *const _,
        cont_arena: &cont_arena as *const _,
    };

    // FoldState bridges the pool's type-erased Job → typed worker code.
    let state = FoldState {
        ctx: &ctx,
        deques: deques.as_slice() as *const [WorkerDeque<FunnelTask<N, H, R>>],
    };
    let job = Job {
        call: worker_entry::<N, H, R, F, G>,
        data: &state as *const FoldState<N, H, R, F, G> as *const (),
    };

    pool.dispatch(&job, || {
        let caller_idx = view.n_workers;
        let caller_deque = &deques[caller_idx];

        walk_cps(&ctx, root.clone(), Cont::Root(root_cell.clone()), caller_deque);

        let mut spins = 0u64;
        while !root_cell.is_done() {
            if let Some(task) = caller_deque.pop() {
                execute_task(&ctx, task, caller_deque);
                spins = 0;
            } else if let Some(task) = steal_from_others(&deques, caller_idx) {
                execute_task(&ctx, task, caller_deque);
                spins = 0;
            } else {
                spins += 1;
                if spins > 10_000_000 {
                    panic!("run_fold hung: root_done={}", root_cell.is_done());
                }
                std::hint::spin_loop();
            }
        }

        // CPS guarantees all work is done. fold_done was set by
        // fire_cont(Cont::Root). Workers exit promptly.
        view.wait_for_workers_to_exit();

        root_cell.take()
    })
}
