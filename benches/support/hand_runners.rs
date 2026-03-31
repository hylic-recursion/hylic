//! Handrolled baselines.
//!
//! Two families:
//!
//! "hand-*" — mirrors the Fold pattern (calls work.do_init/do_accumulate/do_finalize).
//!   Shows hylic's framework overhead vs the same structured decomposition.
//!
//! "real-*" — what a developer would actually write: one flat recursive function,
//!   all logic inlined. No init/accumulate/finalize separation, no WorkSpec methods.
//!   The honest "could I just write this myself?" baseline.

use std::sync::Arc;
use std::hint::black_box;
use hylic::prelude::WorkPool;

use super::tree::NodeId;
use super::work::{WorkSpec, busy_work, spin_wait_us};
use super::scenario::PreparedScenario;

// ── "hand-*": structured baselines (mirror Fold pattern) ───

pub fn handrolled_seq(s: &PreparedScenario) -> u64 {
    fn recurse(children: &[Vec<NodeId>], work: &WorkSpec, node: NodeId) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        for &child in &children[node] {
            let r = recurse(children, work, child);
            work.do_accumulate(&mut heap, &r);
        }
        work.do_finalize(&heap)
    }
    recurse(&s.children, &s.work, s.root)
}

pub fn handrolled_rayon(s: &PreparedScenario) -> u64 {
    use rayon::prelude::*;

    fn recurse(children: &Arc<Vec<Vec<NodeId>>>, work: &WorkSpec, node: NodeId) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        let ch = &children[node];
        if ch.len() <= 1 {
            for &child in ch {
                let r = recurse(children, work, child);
                work.do_accumulate(&mut heap, &r);
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

pub fn handrolled_pool(s: &PreparedScenario, pool: &Arc<WorkPool>) -> u64 {
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
        children: &Arc<Vec<Vec<NodeId>>>,
        work: &Arc<WorkSpec>,
        pool: &Arc<WorkPool>,
        node: NodeId,
    ) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        let ch = &children[node];
        let n = ch.len();

        if n <= 1 {
            for &child in ch {
                let r = recurse(children, work, pool, child);
                work.do_accumulate(&mut heap, &r);
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

    let work = Arc::new(s.work.clone());
    recurse(&s.children, &work, pool, s.root)
}

// ── "real-*": what a developer would actually write ────────
//
// One flat recursive function. All work inlined — no WorkSpec
// methods, no init/accumulate/finalize decomposition. Just
// "process node, recurse children, combine results."

pub fn realworld_seq(s: &PreparedScenario) -> u64 {
    let iw = s.work.init_work;
    let aw = s.work.accumulate_work;
    let fw = s.work.finalize_work;
    let gw = s.work.graph_work;
    let gio = s.work.graph_io_us;

    fn recurse(
        children: &[Vec<NodeId>], node: NodeId,
        iw: u64, aw: u64, fw: u64, gw: u64, gio: u64,
    ) -> u64 {
        // "discover children" (graph traversal cost)
        spin_wait_us(gio);
        if gw > 0 { black_box(busy_work(gw)); }

        // "process this node"
        let mut result = if iw > 0 { busy_work(iw) } else { 0 };

        // "recurse and combine"
        for &child in &children[node] {
            let child_result = recurse(children, child, iw, aw, fw, gw, gio);
            if aw > 0 { result = result.wrapping_add(busy_work(aw)); }
            result = result.wrapping_add(child_result);
        }

        // "finalize"
        if fw > 0 { result = result.wrapping_add(busy_work(fw)); }
        result
    }
    recurse(&s.children, s.root, iw, aw, fw, gw, gio)
}

pub fn realworld_rayon(s: &PreparedScenario) -> u64 {
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

// ── Mode dispatch ──────────────────────────────────────────

pub const HAND_MODES: [&str; 5] = [
    "hand-seq", "hand-rayon", "hand-pool",
    "real-seq", "real-rayon",
];

pub fn run_hand(name: &str, s: &PreparedScenario, pool: &Arc<WorkPool>) -> u64 {
    match name {
        "hand-seq"    => handrolled_seq(s),
        "hand-rayon"  => handrolled_rayon(s),
        "hand-pool"   => handrolled_pool(s, pool),
        "real-seq"    => realworld_seq(s),
        "real-rayon"  => realworld_rayon(s),
        _ => panic!("unknown hand mode: {name}"),
    }
}
