//! Stress tests: repeated execution, lifecycle churn.
//! High iteration counts to catch timing-sensitive races.
//! Each test runs for both Default and SharedDefault policies.

use super::*;

fn stress_1500_runs_impl<P: FunnelPolicy>() {
    let tree = big_tree(200, 6);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    Pool::with(nt, |pool| {
        for i in 0..1500 {
            run_on_pool::<P, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &graph, &tree), expected, "iteration {i}");
            });
        }
    });
}

#[test]
fn stress_1500_runs_pw() { stress_1500_runs_impl::<policy::Default>(); }

#[test]
fn stress_1500_runs_sh() { stress_1500_runs_impl::<policy::SharedDefault>(); }

fn stress_1500_runs_adjacency_impl<P: FunnelPolicy>() {
    let adj = gen_adj(200, 8);
    let ch = adj.clone();
    let treeish = crate::graph::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &child in &ch[*n] { cb(&child); }
    });
    let fold = dom::fold(
        |_: &usize| 0u64,
        |h: &mut u64, c: &u64| { *h += c; },
        |h: &u64| *h,
    );
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);
    let nt = n_threads();
    Pool::with(nt, |pool| {
        let spec = Spec::<P>::new(
            nt,
            <P::Queue as WorkStealing>::Spec::default(),
            <P::Accumulate as AccumulateStrategy>::Spec::default(),
            <P::Wake as WakeStrategy>::Spec::default(),
        );
        let exec = dom::exec(spec.attach(pool));
        for i in 0..1500 {
            assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "iteration {i}");
        }
    });
}

#[test]
fn stress_1500_runs_adjacency_pw() { stress_1500_runs_adjacency_impl::<policy::Default>(); }

#[test]
fn stress_1500_runs_adjacency_sh() { stress_1500_runs_adjacency_impl::<policy::SharedDefault>(); }

fn pool_lifecycle_impl<P: FunnelPolicy>() {
    let tree = big_tree(10, 3);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    for _ in 0..5000 {
        with_exec::<P, _>(nt, |exec| {
            assert_eq!(exec.run(&fold, &graph, &tree), expected);
        });
    }
}

#[test]
fn pool_lifecycle_pw() { pool_lifecycle_impl::<policy::Default>(); }

#[test]
fn pool_lifecycle_sh() { pool_lifecycle_impl::<policy::SharedDefault>(); }

// ── High-iteration mixed-policy stress (criterion reproduction) ──
//
// The benchmark uses ONE funnel::Pool shared across all policy variants.
// Criterion runs ~100k+ warmup iterations per cell. When PerWorker cells
// run first and Shared cells follow, the pool threads have been through
// hundreds of thousands of epoch transitions. This test mimics that pattern.

fn noop_fold() -> dom::Fold<usize, u64, u64> {
    dom::fold(
        |_: &usize| 0u64,
        |h: &mut u64, c: &u64| { *h += c; },
        |h: &u64| *h,
    )
}

fn noop_adj() -> (std::sync::Arc<Vec<Vec<usize>>>, crate::graph::Treeish<usize>) {
    let adj = gen_adj(200, 8);
    let ch = adj.clone();
    let treeish = crate::graph::treeish_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &child in &ch[*n] { cb(&child); }
    });
    (adj, treeish)
}

/// Same pool, PerWorker first (10k), then Shared (10k).
#[test]
fn mixed_policy_pw_then_sh() {
    let nt = n_threads();
    let fold = noop_fold();
    let (_adj, treeish) = noop_adj();
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    Pool::with(nt, |pool| {
        for i in 0..10_000 {
            run_on_pool::<policy::Default, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw iteration {i}");
            });
        }
        for i in 0..10_000 {
            run_on_pool::<policy::SharedDefault, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh iteration {i}");
            });
        }
    });
}

/// All four policy axes on the same pool, 5k each.
#[test]
fn mixed_policy_all_axes() {
    let nt = n_threads();
    let fold = noop_fold();
    let (_adj, treeish) = noop_adj();
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    Pool::with(nt, |pool| {
        for i in 0..5_000 {
            run_on_pool::<policy::Default, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw.final {i}");
            });
        }
        for i in 0..5_000 {
            run_on_pool::<policy::PerWorkerArrival, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw.arrive {i}");
            });
        }
        for i in 0..5_000 {
            run_on_pool::<policy::SharedDefault, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh.final {i}");
            });
        }
        for i in 0..5_000 {
            run_on_pool::<policy::WideLight, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh.arrive {i}");
            });
        }
    });
}

/// Shared noop 20k — dispatch lifecycle stress.
#[test]
fn shared_noop_stress() {
    let nt = n_threads();
    let fold = noop_fold();
    let (_adj, treeish) = noop_adj();
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    Pool::with(nt, |pool| {
        for i in 0..20_000 {
            run_on_pool::<policy::SharedDefault, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "iteration {i}");
            });
        }
    });
}

/// Shared + OnArrival noop 20k.
#[test]
fn shared_arrive_noop_stress() {
    let nt = n_threads();
    let fold = noop_fold();
    let (_adj, treeish) = noop_adj();
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    Pool::with(nt, |pool| {
        for i in 0..20_000 {
            run_on_pool::<policy::WideLight, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "iteration {i}");
            });
        }
    });
}

/// Interleaved policies, 10k total.
#[test]
fn mixed_policy_interleaved() {
    let nt = n_threads();
    let fold = noop_fold();
    let (_adj, treeish) = noop_adj();
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    Pool::with(nt, |pool| {
        for i in 0..2_500 {
            run_on_pool::<policy::Default, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw.final {i}");
            });
            run_on_pool::<policy::PerWorkerArrival, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw.arrive {i}");
            });
            run_on_pool::<policy::SharedDefault, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh.final {i}");
            });
            run_on_pool::<policy::WideLight, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh.arrive {i}");
            });
        }
    });
}

/// High-iteration folds then pool create/destroy cycles.
#[test]
fn mixed_then_lifecycle() {
    let nt = n_threads();
    let fold = noop_fold();
    let (_adj, treeish) = noop_adj();
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    Pool::with(nt, |pool| {
        for i in 0..10_000 {
            run_on_pool::<policy::Default, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw {i}");
            });
        }
        for i in 0..10_000 {
            run_on_pool::<policy::SharedDefault, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh {i}");
            });
        }
    });

    let tree = big_tree(10, 3);
    let sum = sum_fold();
    let graph = n_graph();
    let exp = dom::FUSED.run(&sum, &graph, &tree);
    for i in 0..1_000 {
        with_exec::<policy::Default, _>(nt, |exec| {
            assert_eq!(exec.run(&sum, &graph, &tree), exp, "lifecycle {i}");
        });
    }
}

/// Diagnostic: how many threads actually participate in a single fold?
/// Instruments init() to record thread IDs via AtomicU64 bitmask.
#[test]
fn thread_participation_diagnostic() {
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    let nt = n_threads();
    let (_adj, treeish) = noop_adj();

    // Bitmask: each thread sets its bit when it calls init()
    let participation = Arc::new(AtomicU64::new(0));

    // Fold that records which thread called init()
    let p = participation.clone();
    let fold = dom::fold(
        move |_: &usize| -> u64 {
            let tid = std::thread::current().id();
            // SAFETY: ThreadId is repr(transparent) over NonZeroU64;
            // test-only reinterpretation to derive a per-thread bit.
            let tid_bits = unsafe { std::mem::transmute::<_, u64>(tid) };
            // Use low bits of tid as a hash into 64-bit mask
            let bit = (tid_bits % 64) as u32;
            p.fetch_or(1u64 << bit, std::sync::atomic::Ordering::Relaxed);
            0u64
        },
        |h: &mut u64, c: &u64| { *h += c; },
        |h: &u64| *h,
    );

    // Run ONE fold and check participation
    let spec = Spec::<policy::Default>::default(nt);
    eprintln!("pool threads: {nt}");

    // Single fold — how many threads participate?
    Pool::with(nt, |pool| {
        participation.store(0, std::sync::atomic::Ordering::Relaxed);
        let exec = dom::exec(spec.attach(pool));
        let _result = exec.run(&fold, &treeish, &0usize);
        let mask = participation.load(std::sync::atomic::Ordering::Relaxed);
        let threads_used = mask.count_ones();
        eprintln!("single fold: {threads_used} threads participated (mask={mask:#066b})");

        // 100 folds — cumulative
        participation.store(0, std::sync::atomic::Ordering::Relaxed);
        for _ in 0..100 {
            exec.run(&fold, &treeish, &0usize);
        }
        let mask = participation.load(std::sync::atomic::Ordering::Relaxed);
        let threads_used = mask.count_ones();
        eprintln!("100 folds: {threads_used} distinct threads participated (mask={mask:#066b})");
    });
}

/// Interleaved: alternate policies every iteration. 50k total.
#[test]
fn mixed_policy_interleaved_50k() {
    let nt = n_threads();
    let fold = noop_fold();
    let (_adj, treeish) = noop_adj();
    let expected = dom::FUSED.run(&fold, &treeish, &0usize);

    Pool::with(nt, |pool| {
        for i in 0..12_500 {
            run_on_pool::<policy::Default, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw.final {i}");
            });
            run_on_pool::<policy::PerWorkerArrival, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "pw.arrive {i}");
            });
            run_on_pool::<policy::SharedDefault, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh.final {i}");
            });
            run_on_pool::<policy::WideLight, _>(pool, nt, |exec| {
                assert_eq!(exec.run(&fold, &treeish, &0usize), expected, "sh.arrive {i}");
            });
        }
    });
}
