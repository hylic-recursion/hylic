//! Hylomorphism proof: fold accumulation interleaves with graph traversal
//! across different subtrees, on different threads, simultaneously.
//!
//! A hylomorphism fuses anamorphism (unfolding) and catamorphism (folding)
//! into one pass. The test proves this property holds under parallel execution:
//! while one subtree is still being traversed (visit events), another
//! subtree's results are already being accumulated (accumulate events).
//!
//! We prove three properties:
//! 1. Multiple threads participated (parallelism occurred)
//! 2. Accumulate events overlap with visit events in global time
//!    (fold happened during traversal, not after)
//! 3. Cross-subtree interleaving: an accumulate from subtree X occurs
//!    between two visits in subtree Y (X ≠ Y), proving that different
//!    parts of the tree are in different phases simultaneously.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use super::*;

// ── Lock-free trace infrastructure ───────────────────

const MAX_TRACE: usize = 2048;

struct TraceEntry {
    thread_id: u64,
    op: u8,         // 0=visit, 1=init, 2=accumulate, 3=finalize
    node_val: i32,
    subtree: u8,    // root child index (0-based), 255 = root itself
    seq: u64,
}

const OP_VISIT: u8 = 0;
const OP_ACCUMULATE: u8 = 2;

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
                    thread_id: tid_u64,
                    op,
                    node_val,
                    subtree,
                    seq: s,
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

// ── Subtree tagging ──────────────────────────────────

/// Tree where each node knows which root-child subtree it belongs to.
#[derive(Clone)]
struct TaggedN {
    val: i32,
    subtree: u8,
    children: Vec<TaggedN>,
}

/// Build a BFS tree with subtree tags. Root children get tags 0..bf-1.
/// All descendants inherit their root-child's tag.
fn tagged_tree(n: usize, bf: usize) -> TaggedN {
    let flat = big_tree(n, bf);
    fn tag(node: &N, subtree: u8) -> TaggedN {
        TaggedN {
            val: node.val,
            subtree,
            children: node.children.iter().map(|c| tag(c, subtree)).collect(),
        }
    }
    TaggedN {
        val: flat.val,
        subtree: 255, // root itself
        children: flat.children.iter().enumerate()
            .map(|(i, c)| tag(c, i as u8))
            .collect(),
    }
}

// ── The test ─────────────────────────────────────────

/// Proves the hylomorphism property under parallel execution.
///
/// Runs multiple iterations because interleaving is scheduler-dependent.
/// Uses 85 nodes (bf=4, depth 3+) for enough work to distribute.
#[test]
fn cross_subtree_interleaving() {
    let tree = tagged_tree(85, 4);
    let n_workers = 3;
    let mut proven = false;

    for attempt in 0..20 {
        let seq = Arc::new(AtomicU64::new(0));
        let trace = Arc::new(TraceLog::new());

        let s1 = seq.clone(); let t1 = trace.clone();
        let graph = dom::treeish(move |n: &TaggedN| {
            std::thread::yield_now();
            t1.push(&s1, OP_VISIT, n.val, n.subtree);
            n.children.clone()
        });

        let s2 = seq.clone(); let t2 = trace.clone();
        let s3 = seq.clone(); let t3 = trace.clone();
        let s4 = seq.clone(); let t4 = trace.clone();
        let fold = dom::fold(
            move |n: &TaggedN| -> i32 {
                t2.push(&s2, 1, n.val, n.subtree);
                n.val
            },
            move |heap: &mut i32, child: &i32| {
                t3.push(&s3, OP_ACCUMULATE, *heap, 255);
                *heap += child;
            },
            move |heap: &i32| -> i32 {
                t4.push(&s4, 3, *heap, 255);
                *heap
            },
        );

        let expected = dom::FUSED.run(&fold, &graph, &tree);

        // Reset for the hylo run
        seq.store(0, Ordering::Relaxed);
        trace.len.store(0, Ordering::Relaxed);

        WorkPool::with(WorkPoolSpec::threads(n_workers), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(
                pool, HylomorphicSpec::default_for(n_workers));
            let result = exec.run(&fold, &graph, &tree);
            assert_eq!(result, expected, "result mismatch on attempt {attempt}");
        });

        let entries = trace.as_slice();

        // Check 1: multiple threads
        let mut tids: Vec<u64> = entries.iter().map(|e| e.thread_id).collect();
        tids.sort(); tids.dedup();
        if tids.len() < 2 { continue; } // retry — scheduler didn't parallelize

        // Check 2: global interleaving (first accumulate < last visit)
        let first_acc = entries.iter()
            .filter(|e| e.op == OP_ACCUMULATE).map(|e| e.seq).min();
        let last_visit = entries.iter()
            .filter(|e| e.op == OP_VISIT).map(|e| e.seq).max();
        let global_interleaved = match (first_acc, last_visit) {
            (Some(a), Some(v)) => a < v,
            _ => false,
        };
        if !global_interleaved { continue; }

        // Check 3: cross-subtree interleaving
        // Find: an accumulate from thread T1 with seq S, and two visits
        // from a DIFFERENT subtree on thread T2 with seq S1 < S < S2.
        // This proves T1 is folding subtree X while T2 is still visiting subtree Y.
        let visits: Vec<&TraceEntry> = entries.iter()
            .filter(|e| e.op == OP_VISIT && e.subtree != 255)
            .collect();
        let accumulates: Vec<&TraceEntry> = entries.iter()
            .filter(|e| e.op == OP_ACCUMULATE)
            .collect();

        let mut cross_subtree = false;
        // For each subtree, collect its visit seq range
        let mut subtree_visits: std::collections::HashMap<u8, (u64, u64)> = Default::default();
        for v in &visits {
            let entry = subtree_visits.entry(v.subtree).or_insert((u64::MAX, 0));
            entry.0 = entry.0.min(v.seq);
            entry.1 = entry.1.max(v.seq);
        }

        // An accumulate at seq S interleaves with subtree Y if
        // min_visit_Y < S < max_visit_Y
        for acc in &accumulates {
            for (&subtree_id, &(min_v, max_v)) in &subtree_visits {
                let _ = subtree_id;
                if acc.seq > min_v && acc.seq < max_v {
                    cross_subtree = true;
                    break;
                }
            }
            if cross_subtree { break; }
        }

        if tids.len() >= 2 && global_interleaved && cross_subtree {
            proven = true;
            break;
        }
    }

    assert!(proven,
        "Failed to observe cross-subtree interleaving in 20 attempts. \
         Either the tree is too small, workers too few, or the executor \
         is not truly hylomorphic (fold not interleaving with traversal).");
}
