//! Benchmark modes — sequential and parallel, DRY.
//!
//! Two builder functions: `sequential_modes` and `parallel_modes`.
//! Each returns a Vec<BenchMode> with pre-built closures.
//! Hylic modes and handrolled baselines are grouped by parallelism,
//! not by framework.

use std::sync::Arc;
use std::hint::black_box;
use hylic::domain::shared::{self as dom, Executor, ExecutorExt};
use hylic::prelude::{ParLazy, ParEager, WorkPool};

use super::tree::NodeId;
use super::work::{WorkSpec, busy_work, spin_wait_us};
use super::scenario::PreparedScenario;

/// A pre-built benchmark mode: name + runner closure.
pub struct BenchMode<'a, R> {
    pub name: &'static str,
    pub run: Box<dyn Fn() -> R + 'a>,
}

// ══════════════════════════════════════════════════
// Sequential modes — no threads, no pool, no rayon
// ══════════════════════════════════════════════════

pub fn sequential_modes<'a>(s: &'a PreparedScenario) -> Vec<BenchMode<'a, u64>> {
    let fold = &s.fold;
    let treeish = &s.treeish;
    let root = &s.root;

    // Local domain: single layer of Rc, from WorkSpec
    let local_fold = {
        let w1 = s.work.clone();
        let w2 = s.work.clone();
        let w3 = s.work.clone();
        hylic::domain::local::fold(
            move |_: &NodeId| w1.do_init(),
            move |h: &mut u64, c: &u64| w2.do_accumulate(h, c),
            move |h: &u64| w3.do_finalize(h),
        )
    };
    let local_treeish = {
        let w = s.work.clone();
        let ch = s.children.clone();
        hylic::domain::local::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
            w.do_graph();
            for &child in &ch[*n] { cb(&child); }
        })
    };

    // Owned domain: single layer of Box, pre-built
    let owned_fold = {
        let w1 = s.work.clone();
        let w2 = s.work.clone();
        let w3 = s.work.clone();
        hylic::domain::owned::fold(
            move |_: &NodeId| w1.do_init(),
            move |h: &mut u64, c: &u64| w2.do_accumulate(h, c),
            move |h: &u64| w3.do_finalize(h),
        )
    };
    let owned_treeish = {
        let w = s.work.clone();
        let ch = s.children.clone();
        hylic::domain::owned::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
            w.do_graph();
            for &child in &ch[*n] { cb(&child); }
        })
    };

    vec![
        // ── hylic sequential ───────────────────────
        BenchMode { name: "hylic-fused",
            run: Box::new(move || dom::FUSED.run(fold, treeish, root)) },
        BenchMode { name: "hylic-fused-local",
            run: Box::new(move || hylic::domain::local::FUSED.run(&local_fold, &local_treeish, root)) },
        BenchMode { name: "hylic-fused-owned",
            run: Box::new(move || hylic::domain::owned::FUSED.run(&owned_fold, &owned_treeish, root)) },
        BenchMode { name: "hylic-sequential",
            run: Box::new(move || dom::SEQUENTIAL.run(fold, treeish, root)) },

        // ── handrolled sequential ──────────────────
        BenchMode { name: "hand-seq",
            run: Box::new(|| handrolled_seq(s)) },
        BenchMode { name: "real-seq",
            run: Box::new(|| realworld_seq(s)) },
    ]
}

// ══════════════════════════════════════════════════
// Parallel modes — rayon, lifts, WorkPool
// ══════════════════════════════════════════════════

pub fn parallel_modes<'a>(
    s: &'a PreparedScenario,
    pool: &'a Arc<WorkPool>,
) -> Vec<BenchMode<'a, u64>> {
    let fold = &s.fold;
    let treeish = &s.treeish;
    let root = &s.root;

    let par_lazy = ParLazy::lift::<NodeId, u64, u64>();
    let par_lazy2 = ParLazy::lift::<NodeId, u64, u64>();
    let par_eager_fused = ParEager::lift::<NodeId, u64, u64>(pool);
    let par_eager_rayon = ParEager::lift::<NodeId, u64, u64>(pool);

    let work = Arc::new(s.work.clone());

    vec![
        // ── hylic parallel ─────────────────────────
        BenchMode { name: "hylic-rayon",
            run: Box::new(move || dom::RAYON.run(fold, treeish, root)) },
        BenchMode { name: "hylic-parref+fused",
            run: Box::new(move || dom::FUSED.run_lifted(&par_lazy, fold, treeish, root)) },
        BenchMode { name: "hylic-parref+rayon",
            run: Box::new(move || dom::RAYON.run_lifted(&par_lazy2, fold, treeish, root)) },
        BenchMode { name: "hylic-eager+fused",
            run: Box::new(move || dom::FUSED.run_lifted(&par_eager_fused, fold, treeish, root)) },
        BenchMode { name: "hylic-eager+rayon",
            run: Box::new(move || dom::RAYON.run_lifted(&par_eager_rayon, fold, treeish, root)) },

        // ── handrolled parallel ────────────────────
        BenchMode { name: "hand-rayon",
            run: Box::new(|| handrolled_rayon(s)) },
        BenchMode { name: "hand-pool",
            run: Box::new(move || handrolled_pool(&s.children, &work, pool, s.root)) },
        BenchMode { name: "real-rayon",
            run: Box::new(|| realworld_rayon(s)) },
    ]
}

// ══════════════════════════════════════════════════
// Handrolled recursion engines (private)
// ══════════════════════════════════════════════════

fn handrolled_seq(s: &PreparedScenario) -> u64 {
    fn recurse(children: &[Vec<NodeId>], work: &WorkSpec, node: NodeId) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        for &child in &children[node] {
            work.do_accumulate(&mut heap, &recurse(children, work, child));
        }
        work.do_finalize(&heap)
    }
    recurse(&s.children, &s.work, s.root)
}

fn handrolled_rayon(s: &PreparedScenario) -> u64 {
    use rayon::prelude::*;
    fn recurse(children: &Arc<Vec<Vec<NodeId>>>, work: &WorkSpec, node: NodeId) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        let ch = &children[node];
        if ch.len() <= 1 {
            for &child in ch {
                work.do_accumulate(&mut heap, &recurse(children, work, child));
            }
        } else {
            let results: Vec<u64> = ch.par_iter()
                .map(|&c| recurse(children, work, c))
                .collect();
            for r in &results { work.do_accumulate(&mut heap, r); }
        }
        work.do_finalize(&heap)
    }
    recurse(&s.children, &s.work, s.root)
}

fn handrolled_pool(
    children: &Arc<Vec<Vec<NodeId>>>,
    work: &Arc<WorkSpec>,
    pool: &Arc<WorkPool>,
    root: NodeId,
) -> u64 {
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ForkResults {
        slots: Vec<UnsafeCell<Option<u64>>>,
        remaining: AtomicUsize,
    }
    unsafe impl Sync for ForkResults {}
    impl ForkResults {
        fn new(n: usize) -> Self {
            ForkResults {
                slots: (0..n).map(|_| UnsafeCell::new(None)).collect(),
                remaining: AtomicUsize::new(n),
            }
        }
        unsafe fn write(&self, i: usize, v: u64) {
            unsafe { *self.slots[i].get() = Some(v); }
            self.remaining.fetch_sub(1, Ordering::Release);
        }
        fn is_done(&self) -> bool { self.remaining.load(Ordering::Acquire) == 0 }
        unsafe fn get(&self, i: usize) -> u64 {
            unsafe { (*self.slots[i].get()).unwrap() }
        }
    }

    fn recurse(
        children: &Arc<Vec<Vec<NodeId>>>, work: &Arc<WorkSpec>,
        pool: &Arc<WorkPool>, node: NodeId,
    ) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        let ch = &children[node];
        let n = ch.len();
        if n <= 1 {
            for &child in ch {
                work.do_accumulate(&mut heap, &recurse(children, work, pool, child));
            }
        } else {
            let results = Arc::new(ForkResults::new(n - 1));
            for i in 0..n - 1 {
                let child = ch[i];
                let children_c = children.clone();
                let work_c = work.clone();
                let pool_c = pool.clone();
                let results_c = results.clone();
                pool.submit(Box::new(move || {
                    let r = recurse(&children_c, &work_c, &pool_c, child);
                    unsafe { results_c.write(i, r); }
                }));
            }
            let last_r = recurse(children, work, pool, ch[n - 1]);
            while !results.is_done() {
                if !pool.try_run_one() { std::hint::spin_loop(); }
            }
            for i in 0..n - 1 {
                let r = unsafe { results.get(i) };
                work.do_accumulate(&mut heap, &r);
            }
            work.do_accumulate(&mut heap, &last_r);
        }
        work.do_finalize(&heap)
    }
    recurse(children, work, pool, root)
}

fn realworld_seq(s: &PreparedScenario) -> u64 {
    let iw = s.work.init_work;
    let aw = s.work.accumulate_work;
    let fw = s.work.finalize_work;
    let gw = s.work.graph_work;
    let gio = s.work.graph_io_us;
    fn recurse(
        children: &[Vec<NodeId>], node: NodeId,
        iw: u64, aw: u64, fw: u64, gw: u64, gio: u64,
    ) -> u64 {
        spin_wait_us(gio);
        if gw > 0 { black_box(busy_work(gw)); }
        let mut result = if iw > 0 { busy_work(iw) } else { 0 };
        for &child in &children[node] {
            let child_result = recurse(children, child, iw, aw, fw, gw, gio);
            if aw > 0 { result = result.wrapping_add(busy_work(aw)); }
            result = result.wrapping_add(child_result);
        }
        if fw > 0 { result = result.wrapping_add(busy_work(fw)); }
        result
    }
    recurse(&s.children, s.root, iw, aw, fw, gw, gio)
}

fn realworld_rayon(s: &PreparedScenario) -> u64 {
    use rayon::prelude::*;
    let iw = s.work.init_work;
    let aw = s.work.accumulate_work;
    let fw = s.work.finalize_work;
    let gw = s.work.graph_work;
    let gio = s.work.graph_io_us;
    fn recurse(
        children: &Arc<Vec<Vec<NodeId>>>, node: NodeId,
        iw: u64, aw: u64, fw: u64, gw: u64, gio: u64,
    ) -> u64 {
        spin_wait_us(gio);
        if gw > 0 { black_box(busy_work(gw)); }
        let mut result = if iw > 0 { busy_work(iw) } else { 0 };
        let ch = &children[node];
        if ch.len() <= 1 {
            for &child in ch {
                let child_result = recurse(children, child, iw, aw, fw, gw, gio);
                if aw > 0 { result = result.wrapping_add(busy_work(aw)); }
                result = result.wrapping_add(child_result);
            }
        } else {
            let results: Vec<u64> = ch.par_iter()
                .map(|&c| recurse(children, c, iw, aw, fw, gw, gio))
                .collect();
            for r in &results {
                if aw > 0 { result = result.wrapping_add(busy_work(aw)); }
                result = result.wrapping_add(*r);
            }
        }
        if fw > 0 { result = result.wrapping_add(busy_work(fw)); }
        result
    }
    recurse(&s.children, s.root, iw, aw, fw, gw, gio)
}
