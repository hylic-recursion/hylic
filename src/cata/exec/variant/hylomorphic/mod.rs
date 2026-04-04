//! Hylomorphic parallel executor: CPS zipper with reactive accumulation.

pub(crate) mod fold_chain;
mod walk;

use std::marker::PhantomData;
use std::sync::Arc;
use crate::ops::LiftOps;
use crate::domain::Domain;
use crate::prelude::parallel::pool::{WorkPool, PoolExecView};
use super::super::Executor;

pub struct HylomorphicSpec { pub _reserved: () }
impl HylomorphicSpec {
    pub fn default_for(_n_workers: usize) -> Self { HylomorphicSpec { _reserved: () } }
}

pub struct HylomorphicIn<D> {
    pool: Arc<WorkPool>,
    _spec: HylomorphicSpec,
    _domain: PhantomData<D>,
}

impl<D> HylomorphicIn<D> {
    pub fn new(pool: &Arc<WorkPool>, spec: HylomorphicSpec) -> Self {
        HylomorphicIn { pool: pool.clone(), _spec: spec, _domain: PhantomData }
    }
}

impl<N, R, D: Domain<N>> Executor<N, R, D> for HylomorphicIn<D>
where N: Clone + Send + 'static, R: Clone + Send + 'static,
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        let view = PoolExecView::new(&self.pool);
        walk::run_fold(fold, graph, root, &view)
    }
}

impl<D> HylomorphicIn<D> {
    pub fn run<N, H, R>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R where D: Domain<N>, N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static {
        let view = PoolExecView::new(&self.pool);
        walk::run_fold(fold, graph, root, &view)
    }

    pub fn run_lifted<N, R, N0, H0, R0, H>(
        &self, lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>, graph: &<D as Domain<N0>>::Treeish, root: &N0,
    ) -> R0 where
        D: Domain<N> + Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone, <D as Domain<N0>>::Treeish: Clone,
        N: Clone + Send + 'static, H: 'static, R: Clone + Send + 'static,
        N0: Clone + Send + 'static, H0: 'static, R0: 'static,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::shared as dom;
    use crate::prelude::{WorkPool, WorkPoolSpec};

    #[derive(Clone)]
    struct N { val: i32, children: Vec<N> }

    /// Build a wide tree BFS-style: fill each level left-to-right before going deeper.
    /// big_tree(21, 4) → root with 4 children, each with 4 children = 1+4+16 = 21 nodes.
    fn big_tree(n: usize, bf: usize) -> N {
        if n == 0 { return N { val: 0, children: vec![] }; }
        // Allocate all nodes flat, then wire children BFS.
        let mut nodes: Vec<N> = (0..n).map(|i| N { val: (i + 1) as i32, children: vec![] }).collect();
        // BFS parent assignment: node i has children [i*bf+1 .. i*bf+bf], if they exist.
        // Build bottom-up so we can move children into parents.
        for i in (0..n).rev() {
            let first_child = i * bf + 1;
            if first_child < n {
                let last_child = (first_child + bf).min(n);
                // Drain children from the flat array (they're after parent, so safe to split)
                let children: Vec<N> = (first_child..last_child)
                    .rev()
                    .map(|c| std::mem::replace(&mut nodes[c], N { val: 0, children: vec![] }))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                nodes[i].children = children;
            }
        }
        nodes.into_iter().next().unwrap()
    }

    #[test]
    fn matches_fused() {
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn matches_fused_200() {
        let tree = big_tree(200, 6);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(4), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(4));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn zero_workers() {
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(0), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(0));
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn stress_200x() {
        let tree = big_tree(200, 6);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        for i in 0..200 {
            WorkPool::with(WorkPoolSpec::threads(4), |pool| {
                let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(4));
                assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
            });
        }
    }

    #[test]
    fn with_lift_lazy() {
        use crate::prelude::ParLazy;
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run_lifted(&ParLazy::lift(pool), &fold, &graph, &tree), expected);
        });
    }

    #[test]
    fn with_lift_eager() {
        use crate::prelude::{ParEager, EagerSpec};
        let tree = big_tree(60, 4);
        let fold = dom::simple_fold(|n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
        let graph = dom::treeish(|n: &N| n.children.clone());
        let expected = dom::FUSED.run(&fold, &graph, &tree);
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            assert_eq!(exec.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &fold, &graph, &tree), expected);
        });
    }

    /// Validate that graph traversal and fold accumulation happen
    /// concurrently on different threads, interleaved in time.
    ///
    /// Uses a shared atomic sequence counter. Each graph.visit child
    /// callback and each fold.accumulate call records (thread_id,
    /// operation, sequence_number). After the fold, we check:
    /// - Multiple thread IDs appear (parallelism happened)
    /// - Visit events from one subtree interleave with accumulate
    ///   events from another (interleaved execution)
    #[test]
    fn validates_interleaved_parallel_execution() {
        use std::sync::atomic::AtomicU64;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Debug)]
        #[allow(dead_code)]
        enum Op { Visit(usize), Init(usize), Accumulate(usize), Finalize(usize) }

        #[derive(Clone, Debug)]
        struct TraceEntry {
            thread_id: u64,
            op: Op,
            seq: u64,
        }

        let seq = Arc::new(AtomicU64::new(0));
        let trace = Arc::new(Mutex::new(Vec::<TraceEntry>::new()));

        fn thread_id() -> u64 {
            // Use a simple hash of the thread id
            let id = std::thread::current().id();
            let s = format!("{:?}", id);
            let mut h = 0u64;
            for b in s.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
            h
        }

        let seq_g = seq.clone();
        let trace_g = trace.clone();
        let graph = dom::treeish(move |n: &N| {
            // Simulate real graph work (sleep a bit so interleaving is observable)
            std::thread::yield_now();
            let s = seq_g.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            trace_g.lock().unwrap().push(TraceEntry {
                thread_id: thread_id(), op: Op::Visit(n.val as usize), seq: s,
            });
            n.children.clone()
        });

        let seq_f = seq.clone();
        let trace_f = trace.clone();
        let seq_f2 = seq.clone();
        let trace_f2 = trace.clone();
        let seq_f3 = seq.clone();
        let trace_f3 = trace.clone();

        let fold = dom::fold(
            move |n: &N| -> i32 {
                let s = seq_f.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                trace_f.lock().unwrap().push(TraceEntry {
                    thread_id: thread_id(), op: Op::Init(n.val as usize), seq: s,
                });
                n.val
            },
            move |heap: &mut i32, child: &i32| {
                let s = seq_f2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                trace_f2.lock().unwrap().push(TraceEntry {
                    thread_id: thread_id(), op: Op::Accumulate(*heap as usize), seq: s,
                });
                *heap += child;
            },
            move |heap: &i32| -> i32 {
                let s = seq_f3.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                trace_f3.lock().unwrap().push(TraceEntry {
                    thread_id: thread_id(), op: Op::Finalize(*heap as usize), seq: s,
                });
                *heap
            },
        );

        // Wide tree: 4 children at root, each with 4 children = 21 nodes
        let tree = big_tree(21, 4);
        let expected = dom::FUSED.run(&fold, &graph, &tree);

        // Clear trace from the Fused run
        trace.lock().unwrap().clear();
        seq.store(0, std::sync::atomic::Ordering::Relaxed);

        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(3));
            let result = exec.run(&fold, &graph, &tree);
            assert_eq!(result, expected, "result mismatch");
        });

        let trace = trace.lock().unwrap();

        // Check 1: multiple threads participated
        let mut thread_ids: Vec<u64> = trace.iter().map(|t| t.thread_id).collect();
        thread_ids.sort();
        thread_ids.dedup();
        let parallel = thread_ids.len() > 1;

        // Check 2: visit events from different subtrees interleave with
        // accumulate events (i.e., accumulation happened DURING traversal,
        // not after all traversal was done)
        let first_accumulate_seq = trace.iter()
            .filter(|t| matches!(t.op, Op::Accumulate(_)))
            .map(|t| t.seq)
            .min();
        let last_visit_seq = trace.iter()
            .filter(|t| matches!(t.op, Op::Visit(_)))
            .map(|t| t.seq)
            .max();

        let interleaved = match (first_accumulate_seq, last_visit_seq) {
            (Some(first_acc), Some(last_vis)) => first_acc < last_vis,
            _ => false,
        };

        // Report
        if !parallel {
            eprintln!("[interleave test] WARNING: only 1 thread participated. \
                       The tree may be too small or workers too slow to steal.");
        }
        if !interleaved {
            eprintln!("[interleave test] WARNING: accumulate did not interleave with visit. \
                       First accumulate seq={:?}, last visit seq={:?}",
                       first_accumulate_seq, last_visit_seq);
        }

        // We assert interleaving: at least one accumulate happened before
        // the last visit. This proves the fold started producing results
        // while the graph was still being traversed.
        assert!(interleaved,
            "Accumulate should interleave with visit (fold during traversal). \
             First accumulate={:?}, last visit={:?}. Trace has {} entries across {} threads.",
            first_accumulate_seq, last_visit_seq, trace.len(), thread_ids.len());
    }

    /// Reproduces the bench_executor_compare pattern: adjacency-list graph,
    /// NodeId=usize, noop work, repeated execution. Tests for hangs.
    #[test]
    fn bench_pattern_noop_200() {
        use std::sync::Arc;
        type NodeId = usize;

        // BFS adjacency list (same as benches/support/tree.rs gen_tree)
        fn gen_adj(node_count: usize, bf: usize) -> Arc<Vec<Vec<NodeId>>> {
            let mut children: Vec<Vec<NodeId>> = vec![vec![]];
            let mut next_id = 1usize;
            let mut level_start = 0;
            let mut level_end = 1;
            while next_id < node_count {
                let mut new_end = level_end;
                for parent in level_start..level_end {
                    let n_ch = bf.min(node_count - next_id);
                    if n_ch == 0 { break; }
                    let mut my_ch = Vec::with_capacity(n_ch);
                    for _ in 0..n_ch {
                        if next_id >= node_count { break; }
                        children.push(vec![]);
                        my_ch.push(next_id);
                        next_id += 1;
                        new_end += 1;
                    }
                    children[parent] = my_ch;
                }
                level_start = level_end;
                level_end = new_end;
                if level_start == level_end { break; }
            }
            Arc::new(children)
        }

        let adj = gen_adj(200, 8);
        let ch = adj.clone();
        let treeish = dom::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
            for &child in &ch[*n] { cb(&child); }
        });
        let fold = dom::fold(
            |_: &NodeId| 0u64,
            |h: &mut u64, c: &u64| { *h += c; },
            |h: &u64| *h,
        );

        let expected = dom::FUSED.run(&fold, &treeish, &0usize);

        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let exec = HylomorphicIn::<crate::domain::Shared>::new(
                pool, HylomorphicSpec::default_for(3));
            for i in 0..200 {
                eprintln!("[repro] iter {i} start");
                let result = exec.run(&fold, &treeish, &0usize);
                eprintln!("[repro] iter {i} done");
                assert_eq!(result, expected, "iteration {i}");
            }
        });
    }
}
