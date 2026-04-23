//! Hylomorphism proof: fold interleaves with traversal across subtrees.
//! Same methodology as the hylo interleaving test — lock-free tracing,
//! subtree tagging, cross-subtree assertion.
//! Each test runs for both PerWorker and Shared queue strategies.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use super::*;
use super::super::policy;

const MAX_TRACE: usize = 2048;
const OP_VISIT: u8 = 0;
const OP_ACCUMULATE: u8 = 2;

struct TraceEntry {
    thread_id: u64,
    op: u8,
    node_val: i32,
    subtree: u8,
    seq: u64,
}

struct TraceLog {
    entries: Box<[std::cell::UnsafeCell<std::mem::MaybeUninit<TraceEntry>>; MAX_TRACE]>,
    len: AtomicU64,
}

unsafe impl Send for TraceLog {}
unsafe impl Sync for TraceLog {}

impl TraceLog {
    fn new() -> Self {
        TraceLog {
            entries: Box::new(std::array::from_fn(|_| {
                std::cell::UnsafeCell::new(std::mem::MaybeUninit::uninit())
            })),
            len: AtomicU64::new(0),
        }
    }

    fn push(&self, seq: &AtomicU64, op: u8, node_val: i32, subtree: u8) {
        let s = seq.fetch_add(1, Ordering::Relaxed);
        let idx = self.len.fetch_add(1, Ordering::Relaxed) as usize;
        if idx < MAX_TRACE {
            let tid = std::thread::current().id();
            let tid_u64 = unsafe { std::mem::transmute::<_, u64>(tid) };
            unsafe {
                (*self.entries[idx].get()).write(TraceEntry {
                    thread_id: tid_u64, op, node_val, subtree, seq: s,
                });
            }
        }
    }

    fn as_slice(&self) -> &[TraceEntry] {
        let len = (self.len.load(Ordering::Acquire) as usize).min(MAX_TRACE);
        unsafe {
            std::slice::from_raw_parts(
                self.entries[0].get() as *const TraceEntry,
                len,
            )
        }
    }
}

#[derive(Clone)]
struct TaggedN { val: i32, subtree: u8, children: Vec<TaggedN> }

fn tagged_tree(n: usize, bf: usize) -> TaggedN {
    let flat = big_tree(n, bf);
    fn tag(node: &N, subtree: u8) -> TaggedN {
        TaggedN {
            val: node.val, subtree,
            children: node.children.iter().map(|c| tag(c, subtree)).collect(),
        }
    }
    TaggedN {
        val: flat.val, subtree: 255,
        children: flat.children.iter().enumerate()
            .map(|(i, c)| tag(c, i as u8)).collect(),
    }
}

fn cross_subtree_interleaving_impl<P: FunnelPolicy>() {
    let tree = tagged_tree(85, 4);
    let n_workers = n_threads();
    let mut proven = false;

    let seq = Arc::new(AtomicU64::new(0));
    let trace = Arc::new(TraceLog::new());

    for attempt in 0..20 {
        seq.store(0, Ordering::Relaxed);
        trace.len.store(0, Ordering::Relaxed);

        let s1 = seq.clone(); let t1 = trace.clone();
        let graph = crate::graph::treeish(move |n: &TaggedN| {
            std::thread::yield_now();
            t1.push(&s1, OP_VISIT, n.val, n.subtree);
            n.children.clone()
        });

        let s2 = seq.clone(); let t2 = trace.clone();
        let s3 = seq.clone(); let t3 = trace.clone();
        let s4 = seq.clone(); let t4 = trace.clone();
        let fold = dom::fold(
            move |n: &TaggedN| -> i32 { t2.push(&s2, 1, n.val, n.subtree); n.val },
            move |heap: &mut i32, child: &i32| { t3.push(&s3, OP_ACCUMULATE, *heap, 255); *heap += child; },
            move |heap: &i32| -> i32 { t4.push(&s4, 3, *heap, 255); *heap },
        );

        let expected = dom::FUSED.run(&fold, &graph, &tree);

        seq.store(0, Ordering::Relaxed);
        trace.len.store(0, Ordering::Relaxed);

        let result = with_exec::<P, _>(n_workers, |exec| exec.run(&fold, &graph, &tree));
        assert_eq!(result, expected, "result mismatch on attempt {attempt}");

        let entries = trace.as_slice();

        let mut tids: Vec<u64> = entries.iter().map(|e| e.thread_id).collect();
        tids.sort(); tids.dedup();
        if tids.len() < 2 { continue; }

        let first_acc = entries.iter().filter(|e| e.op == OP_ACCUMULATE).map(|e| e.seq).min();
        let last_visit = entries.iter().filter(|e| e.op == OP_VISIT).map(|e| e.seq).max();
        let global_interleaved = matches!((first_acc, last_visit), (Some(a), Some(v)) if a < v);
        if !global_interleaved { continue; }

        let mut subtree_visits: std::collections::HashMap<u8, (u64, u64)> = Default::default();
        for e in entries.iter().filter(|e| e.op == OP_VISIT && e.subtree != 255) {
            let range = subtree_visits.entry(e.subtree).or_insert((u64::MAX, 0));
            range.0 = range.0.min(e.seq);
            range.1 = range.1.max(e.seq);
        }

        let cross_subtree = entries.iter()
            .filter(|e| e.op == OP_ACCUMULATE)
            .any(|acc| subtree_visits.values().any(|&(min_v, max_v)| acc.seq > min_v && acc.seq < max_v));

        if cross_subtree {
            proven = true;
            break;
        }
    }

    if !proven {
        let op_name = |o: u8| match o { 0 => "visit", 1 => "init", 2 => "acc", 3 => "fin", _ => "?" };
        let entries = trace.as_slice();
        let mut dump = String::new();
        for e in entries.iter().take(60) {
            dump.push_str(&format!(
                "  seq={:3} {:>5} node={:3} subtree={} tid={:#x}\n",
                e.seq, op_name(e.op), e.node_val, e.subtree, e.thread_id,
            ));
        }
        if entries.len() > 60 { dump.push_str(&format!("  ... ({} more)\n", entries.len() - 60)); }
        panic!("Failed to observe cross-subtree interleaving in 20 attempts.\nLast trace ({} entries):\n{dump}", entries.len());
    }
}

#[test]
fn cross_subtree_interleaving_pw() { cross_subtree_interleaving_impl::<policy::Default>(); }

#[test]
fn cross_subtree_interleaving_sh() { cross_subtree_interleaving_impl::<policy::SharedDefault>(); }
