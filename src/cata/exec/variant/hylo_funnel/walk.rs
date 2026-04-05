//! CPS walk for hylo-funnel: defunctionalized tasks, per-worker deques,
//! arena-allocated ChainNodes, Cont::Direct with ContArena.
//!
//! Tasks are data (FunnelTask enum), not closures. Workers pattern-match.
//! No Box<dyn FnOnce>, no TaskRef, no type erasure.
//!
//! Per-worker deques: push is local (no atomic), steal is a rare CAS.
//! The inner DFS traversal runs with zero shared-state contention.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::ops::{FoldOps, TreeOps};
use super::fold_chain::{FoldChain, SlotRef};
use super::arena::{Arena, ArenaIdx};
use super::cont_arena::{ContArena, ContIdx};
use super::deque::WorkerDeque;
use super::pool::{FunnelPoolShared, FunnelPoolSpec, with_pool, steal_from_others};

// ── Debug counters (test only) ───────────────────────

#[cfg(test)]
mod counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    pub static SUBMITTED: AtomicU64 = AtomicU64::new(0);
    pub static WALK_EXEC: AtomicU64 = AtomicU64::new(0);
    pub static RAKE_EXEC: AtomicU64 = AtomicU64::new(0);
    pub static DELIVERIES: AtomicU64 = AtomicU64::new(0);
    pub static FIRE_CONT_CALLS: AtomicU64 = AtomicU64::new(0);
    pub fn reset() {
        SUBMITTED.store(0, Ordering::Relaxed);
        WALK_EXEC.store(0, Ordering::Relaxed);
        RAKE_EXEC.store(0, Ordering::Relaxed);
        DELIVERIES.store(0, Ordering::Relaxed);
        FIRE_CONT_CALLS.store(0, Ordering::Relaxed);
    }
    pub fn inc_submitted() { SUBMITTED.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_walk() { WALK_EXEC.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_rake() { RAKE_EXEC.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_delivery() { DELIVERIES.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_fire_cont() { FIRE_CONT_CALLS.fetch_add(1, Ordering::Relaxed); }
    pub fn snapshot() -> String {
        format!("submitted={}, walk_exec={}, rake_exec={}, deliveries={}, fire_cont={}",
            SUBMITTED.load(Ordering::Relaxed),
            WALK_EXEC.load(Ordering::Relaxed),
            RAKE_EXEC.load(Ordering::Relaxed),
            DELIVERIES.load(Ordering::Relaxed),
            FIRE_CONT_CALLS.load(Ordering::Relaxed),
        )
    }
}

#[cfg(not(test))]
mod counters {
    pub fn reset() {}
    pub fn inc_submitted() {}
    pub fn inc_walk() {}
    pub fn inc_rake() {}
    pub fn inc_delivery() {}
    pub fn inc_fire_cont() {}
    pub fn snapshot() -> String { String::new() }
}

// ── Defunctionalized task ────────────────────────────

pub(super) enum FunnelTask<N, H, R> {
    Walk { child: N, cont: Cont<H, R> },
    Rake { node_idx: ArenaIdx },
}

// Safety: H may not be Send, but it's only accessed by one thread at a time
// (via the FoldChain sweeping CAS gate or inline in Cont::Direct).
// Same safety argument as ChainNode's unsafe impl Send.
unsafe impl<N: Send, H, R: Send> Send for FunnelTask<N, H, R> {}

// ── Lifetime-erased context ───────────────────────────

struct WalkCtx<F, G, H, R> {
    fold: *const F,
    graph: *const G,
    shared: *const FunnelPoolShared,
    chain_arena: *const Arena<ChainNode<H, R>>,
    cont_arena: *const ContArena<Cont<H, R>>,
}

impl<F, G, H, R> Clone for WalkCtx<F, G, H, R> {
    fn clone(&self) -> Self { *self }
}
impl<F, G, H, R> Copy for WalkCtx<F, G, H, R> {}

unsafe impl<F, G, H, R> Send for WalkCtx<F, G, H, R> {}
unsafe impl<F, G, H, R> Sync for WalkCtx<F, G, H, R> {}

impl<F, G, H, R> WalkCtx<F, G, H, R> {
    unsafe fn fold_ref(&self) -> &F { unsafe { &*self.fold } }
    unsafe fn graph_ref(&self) -> &G { unsafe { &*self.graph } }
    unsafe fn shared_ref(&self) -> &FunnelPoolShared { unsafe { &*self.shared } }
    unsafe fn chain_arena(&self) -> &Arena<ChainNode<H, R>> { unsafe { &*self.chain_arena } }
    unsafe fn cont_arena(&self) -> &ContArena<Cont<H, R>> { unsafe { &*self.cont_arena } }
}

// ── Defunctionalized continuation ─────────────────────

enum Cont<H, R> {
    Root(Arc<RootCell<R>>),
    /// Single-child: heap inline, parent by arena index. No FoldChain.
    Direct { heap: H, parent_idx: ContIdx },
    /// Multi-child: result delivered to FoldChain slot.
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

// ── fire_cont (trampolined, with Direct fast path) ────

fn fire_cont<N, H, R, F, G>(
    ctx: &WalkCtx<F, G, H, R>,
    mut cont: Cont<H, R>,
    mut result: R,
    deque: &WorkerDeque<FunnelTask<N, H, R>>,
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
                node.chain.deliver(slot, result);
                counters::inc_delivery();
                let fold = unsafe { ctx.fold_ref() };
                match node.chain.rake(fold) {
                    Some(finalized) => {
                        cont = node.take_parent_cont();
                        result = finalized;
                    }
                    None => {
                        // Submit raker as data — no Box, no closure.
                        let pushed = deque.push(FunnelTask::Rake { node_idx });
                        assert!(pushed, "deque full: raker lost");
                        counters::inc_submitted();
                        let shared = unsafe { ctx.shared_ref() };
                        shared.notify_one();
                        return;
                    }
                }
            }
        }
    }
}

// ── CPS walk ───────────────────────────────────��──────

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
    let shared = unsafe { ctx.shared_ref() };
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
                let cn = ChainNode::new(
                    heap_opt.take().unwrap(),
                    cont_opt.take().unwrap(),
                );
                let idx = chain_arena.alloc(cn);
                let node_ref = unsafe { chain_arena.get(idx) };
                node_ref.chain.append_slot(); // slot 0 for first child
                chain_idx = Some(idx);
            }
            let idx = chain_idx.unwrap();
            let node_ref = unsafe { chain_arena.get(idx) };
            let slot = node_ref.chain.append_slot();
            // Push task as DATA — no Box, no closure.
            let pushed = deque.push(FunnelTask::Walk {
                child: child.clone(),
                cont: Cont::Slot { node: idx, slot },
            });
            assert!(pushed, "deque full: task lost");
            counters::inc_submitted();
            shared.notify_one();
        }
    });

    match child_count {
        0 => {
            // Leaf: finalize and deliver.
            let heap = heap_opt.take().unwrap();
            let cont = cont_opt.take().unwrap();
            let result = fold.finalize(&heap);
            fire_cont::<N, H, R, F, G>(ctx, cont, result, deque);
        }
        1 => {
            // Single child: Cont::Direct — parent cont in arena.
            let child = first_child.unwrap();
            let heap = heap_opt.take().unwrap();
            let parent_cont = cont_opt.take().unwrap();
            let parent_idx = cont_arena.alloc(parent_cont);
            walk_cps::<N, H, R, F, G>(ctx, child, Cont::Direct { heap, parent_idx }, deque);
        }
        _ => {
            // Multi-child: FoldChain with CAS sweep.
            let idx = chain_idx.unwrap();
            let cn = unsafe { chain_arena.get(idx) };
            cn.chain.set_total();
            let child = first_child.unwrap();
            walk_cps::<N, H, R, F, G>(ctx, child, Cont::Slot {
                node: idx,
                slot: SlotRef(0),
            }, deque);
        }
    }
}

// ── Execute a defunctionalized task ──────────────────

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
        FunnelTask::Walk { child, cont } => { counters::inc_walk(); walk_cps(ctx, child, cont, deque) },
        FunnelTask::Rake { node_idx } => { counters::inc_rake();
            let fold = unsafe { ctx.fold_ref() };
            let arena = unsafe { ctx.chain_arena() };
            let node = unsafe { arena.get(node_idx) };
            if let Some(result) = node.chain.rake(fold) {
                let parent = node.take_parent_cont();
                fire_cont::<N, H, R, F, G>(ctx, parent, result, deque);
            }
        }
    }
}

// ── Worker loop ──────────────────────────────���───────

fn worker_loop<N, H, R, F, G>(
    ctx: &WalkCtx<F, G, H, R>,
    shared: &FunnelPoolShared,
    deques: &[WorkerDeque<FunnelTask<N, H, R>>],
    my_idx: usize,
) where
    F: FoldOps<N, H, R> + 'static,
    G: TreeOps<N> + 'static,
    N: Clone + Send + 'static,
    H: 'static,
    R: Send + 'static,
{
    let my_deque = &deques[my_idx];
    loop {
        // Pop from own deque (LIFO — DFS order, no atomic)
        if let Some(task) = my_deque.pop() {
            execute_task(ctx, task, my_deque);
            continue;
        }
        // Steal from neighbors
        if let Some(task) = steal_from_others(deques, my_idx) {
            execute_task(ctx, task, my_deque);
            continue;
        }
        // Park
        let token = shared.event.prepare();
        if shared.shutdown.load(Ordering::Acquire) { return; }
        if let Some(task) = my_deque.pop() {
            execute_task(ctx, task, my_deque);
            continue;
        }
        if let Some(task) = steal_from_others(deques, my_idx) {
            execute_task(ctx, task, my_deque);
            continue;
        }
        shared.idle_count.fetch_add(1, Ordering::Relaxed);
        shared.event.wait(token);
        shared.idle_count.fetch_sub(1, Ordering::Relaxed);
    }
}

// ── Entry point ───────────────────────────────────────

const CHAIN_ARENA_CAPACITY: usize = 4096;
const CONT_ARENA_CAPACITY: usize = 8192;

pub fn run_fold<N, H, R>(
    fold: &(impl FoldOps<N, H, R> + 'static),
    graph: &(impl TreeOps<N> + 'static),
    root: &N,
    n_workers: usize,
) -> R
where
    N: Clone + Send + 'static,
    H: 'static,
    R: Clone + Send + 'static,
{
    counters::reset();
    let chain_arena = Arena::<ChainNode<H, R>>::new(CHAIN_ARENA_CAPACITY);
    let cont_arena = ContArena::<Cont<H, R>>::new(CONT_ARENA_CAPACITY);
    let root_cell = Arc::new(RootCell::new());

    let ctx = WalkCtx {
        fold: fold as *const _,
        graph: graph as *const _,
        shared: std::ptr::null(), // filled below
        chain_arena: &chain_arena as *const _,
        cont_arena: &cont_arena as *const _,
    };

    with_pool(
        FunnelPoolSpec::threads(n_workers),
        |shared, deques| {
            let mut ctx = ctx;
            ctx.shared = shared as *const _;

            let caller_idx = shared.n_workers; // calling thread's deque
            let caller_deque = &deques[caller_idx];

            // Seed the initial walk
            walk_cps(&ctx, root.clone(), Cont::Root(root_cell.clone()), caller_deque);

            // Help-wait loop
            let mut spins = 0u64;
            let mut helped = 0u64;
            while !root_cell.is_done() {
                if let Some(task) = caller_deque.pop() {
                    execute_task(&ctx, task, caller_deque);
                    helped += 1;
                    spins = 0;
                } else if let Some(task) = steal_from_others(deques, caller_idx) {
                    execute_task(&ctx, task, caller_deque);
                    helped += 1;
                    spins = 0;
                } else {
                    spins += 1;
                    if spins > 10_000_000 {
                        let deque_lens: Vec<usize> = deques.iter().map(|d| d.len()).collect();
                        let n_chains = chain_arena.allocated();
                        let mut chain_diags = Vec::new();
                        for i in 0..n_chains {
                            let cn = unsafe { chain_arena.get(ArenaIdx::from_raw(i)) };
                            let diag = cn.chain.diagnostic();
                            if !cn.chain.is_done() {
                                chain_diags.push(format!("  chain[{i}]: {diag} ← INCOMPLETE"));
                            }
                        }
                        panic!(
                            "run_fold hung: root_done={}, helped={}, idle={}, deque_lens={:?}, chains={}, conts={}, {}\n{}",
                            root_cell.is_done(), helped,
                            shared.idle_count.load(Ordering::Relaxed),
                            deque_lens, n_chains, cont_arena.allocated(),
                            counters::snapshot(),
                            chain_diags.join("\n"),
                        );
                    }
                    std::hint::spin_loop();
                }
            }
            root_cell.take()
        },
        |shared, deques, worker_idx| {
            let mut ctx = ctx;
            ctx.shared = shared as *const _;
            worker_loop::<N, H, R, _, _>(&ctx, shared, deques, worker_idx);
        },
    )
}
