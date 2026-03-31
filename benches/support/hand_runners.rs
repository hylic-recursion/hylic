//! Handrolled baselines that reuse the Fold's work but bypass
//! hylic's Treeish/Exec abstractions.

use std::sync::Arc;
use hylic::prelude::WorkPool;

use super::tree::NodeId;
use super::work::WorkSpec;
use super::scenario::PreparedScenario;

/// Sequential recursion — plain function calls on the adjacency list.
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

/// Rayon par_iter on children — the "obvious" parallel version.
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

/// Manual fork-join using WorkPool — same scheduling as ParEager
/// but without hylic's Lift/EagerNode abstraction.
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

pub const HAND_MODES: [&str; 3] = ["hand-seq", "hand-rayon", "hand-pool"];

pub fn run_hand(name: &str, s: &PreparedScenario, pool: &Arc<WorkPool>) -> u64 {
    match name {
        "hand-seq"   => handrolled_seq(s),
        "hand-rayon" => handrolled_rayon(s),
        "hand-pool"  => handrolled_pool(s, pool),
        _ => panic!("unknown hand mode: {name}"),
    }
}
