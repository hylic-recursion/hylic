//! Benchmark modes — sequential and parallel, DRY.
//!
//! Two builder functions: `sequential_modes` and `parallel_modes`.
//! Each returns a Vec<BenchMode> with pre-built closures.
//! Hylic modes and handrolled baselines are grouped by parallelism,
//! not by framework.
//!
//! Config IDs are defined in config.rs — single source of truth.

use std::sync::Arc;
use std::hint::black_box;
use hylic::domain::shared as dom;
use hylic::cata::exec::{pool, funnel};
use hylic::prelude::{ParLazy, ParEager, WorkPool, PoolExecView};

use super::config as id;
use super::tree::NodeId;
use super::work::{WorkSpec, busy_work, spin_wait_us};
use super::scenario::PreparedScenario;

/// A pre-built benchmark mode: name + runner closure.
pub struct BenchMode<'a, R> {
    pub name: &'static str,
    pub run: Box<dyn Fn() -> R + 'a>,
}

// ── Domain fold/treeish constructors ─────────────

fn make_local_fold(work: &WorkSpec) -> hylic::domain::local::Fold<NodeId, u64, u64> {
    let w1 = work.clone();
    let w2 = work.clone();
    let w3 = work.clone();
    hylic::domain::local::fold(
        move |_: &NodeId| w1.do_init(),
        move |h: &mut u64, c: &u64| w2.do_accumulate(h, c),
        move |h: &u64| w3.do_finalize(h),
    )
}

fn make_local_treeish(work: &WorkSpec, children: &Arc<Vec<Vec<NodeId>>>) -> hylic::domain::local::Treeish<NodeId> {
    let w = work.clone();
    let ch = children.clone();
    hylic::domain::local::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
        w.do_graph();
        for &child in &ch[*n] { cb(&child); }
    })
}

fn make_owned_fold(work: &WorkSpec) -> hylic::domain::owned::Fold<NodeId, u64, u64> {
    let w1 = work.clone();
    let w2 = work.clone();
    let w3 = work.clone();
    hylic::domain::owned::fold(
        move |_: &NodeId| w1.do_init(),
        move |h: &mut u64, c: &u64| w2.do_accumulate(h, c),
        move |h: &u64| w3.do_finalize(h),
    )
}

fn make_owned_treeish(work: &WorkSpec, children: &Arc<Vec<Vec<NodeId>>>) -> hylic::domain::owned::Treeish<NodeId> {
    let w = work.clone();
    let ch = children.clone();
    hylic::domain::owned::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
        w.do_graph();
        for &child in &ch[*n] { cb(&child); }
    })
}

// ══════════════════════════════════════════════════
// Sequential modes — no threads, no pool, no rayon
// ══════════════════════════════════════════════════

pub fn sequential_modes<'a>(s: &'a PreparedScenario) -> Vec<BenchMode<'a, u64>> {
    let fold = &s.fold;
    let treeish = &s.treeish;
    let root = &s.root;

    // Local: Clone is cheap (Rc increment)
    let local_fold_1 = make_local_fold(&s.work);
    let local_fold_2 = local_fold_1.clone();
    let local_tree_1 = make_local_treeish(&s.work, &s.children);
    let local_tree_2 = local_tree_1.clone();

    // Owned: not Clone, construct each separately
    let owned_fold_1 = make_owned_fold(&s.work);
    let owned_tree_1 = make_owned_treeish(&s.work, &s.children);
    let owned_fold_2 = make_owned_fold(&s.work);
    let owned_tree_2 = make_owned_treeish(&s.work, &s.children);

    vec![
        // ── hylic fused (all domains) ─────────────────
        BenchMode { name: id::FUSED_SHARED,
            run: Box::new(move || dom::FUSED.run(fold, treeish, root)) },
        BenchMode { name: id::FUSED_LOCAL,
            run: Box::new(move || hylic::domain::local::FUSED.run(&local_fold_1, &local_tree_1, root)) },
        BenchMode { name: id::FUSED_OWNED,
            run: Box::new(move || hylic::domain::owned::FUSED.run(&owned_fold_1, &owned_tree_1, root)) },

        // ── hylic sequential (all domains) ────────────
        BenchMode { name: id::SEQUENTIAL_SHARED,
            run: Box::new(move || dom::SEQUENTIAL.run(fold, treeish, root)) },
        BenchMode { name: id::SEQUENTIAL_LOCAL,
            run: Box::new(move || hylic::domain::local::SEQUENTIAL.run(&local_fold_2, &local_tree_2, root)) },
        BenchMode { name: id::SEQUENTIAL_OWNED,
            run: Box::new(move || hylic::domain::owned::SEQUENTIAL.run(&owned_fold_2, &owned_tree_2, root)) },

        // ── handrolled sequential ─────────────────────
        BenchMode { name: id::HAND_SEQ,
            run: Box::new(|| handrolled_seq(s)) },
        BenchMode { name: id::REAL_SEQ,
            run: Box::new(|| realworld_seq(s)) },
    ]
}

// ══════════════════════════════════════════════════
// Parallel modes — rayon, lifts, WorkPool
// ══════════════════════════════════════════════════

pub fn parallel_modes<'a>(
    s: &'a PreparedScenario,
    pool: &'a Arc<WorkPool>,
    pool_spec: &'a pool::Spec,
) -> Vec<BenchMode<'a, u64>> {
    let fold = &s.fold;
    let treeish = &s.treeish;
    let root = &s.root;

    // Local domain for Pool.Local + lift variants
    let local_fold = make_local_fold(&s.work);
    let local_tree = make_local_treeish(&s.work, &s.children);
    let local_fold_plf = local_fold.clone();
    let local_tree_plf = local_tree.clone();
    let local_fold_pll = local_fold.clone();
    let local_tree_pll = local_tree.clone();
    let local_fold_elf = local_fold.clone();
    let local_tree_elf = local_tree.clone();
    let local_fold_ell = local_fold.clone();
    let local_tree_ell = local_tree.clone();

    // Owned domain for Pool.Owned
    let owned_fold = make_owned_fold(&s.work);
    let owned_tree = make_owned_treeish(&s.work, &s.children);

    // Lifts (Shared domain)
    let par_lazy_fused = ParLazy::lift::<hylic::domain::Shared, NodeId, u64, u64>(pool);
    let par_lazy_rayon = ParLazy::lift::<hylic::domain::Shared, NodeId, u64, u64>(pool);
    let par_lazy_pool  = ParLazy::lift::<hylic::domain::Shared, NodeId, u64, u64>(pool);
    let par_eager_fused = ParEager::lift::<hylic::domain::Shared, NodeId, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(super::config::bench_workers()));
    let par_eager_rayon = ParEager::lift::<hylic::domain::Shared, NodeId, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(super::config::bench_workers()));
    let par_eager_pool  = ParEager::lift::<hylic::domain::Shared, NodeId, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(super::config::bench_workers()));

    // Lifts (Local domain)
    let par_lazy_fused_local  = ParLazy::lift::<hylic::domain::Local, NodeId, u64, u64>(pool);
    let par_lazy_pool_local   = ParLazy::lift::<hylic::domain::Local, NodeId, u64, u64>(pool);
    let par_eager_fused_local = ParEager::lift::<hylic::domain::Local, NodeId, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(super::config::bench_workers()));
    let par_eager_pool_local  = ParEager::lift::<hylic::domain::Local, NodeId, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(super::config::bench_workers()));

    // Pool executors
    let pool_shared  = pool::Exec::<hylic::domain::Shared>::from_pool(pool, pool_spec);
    let pool_shared2 = pool::Exec::<hylic::domain::Shared>::from_pool(pool, pool_spec);
    let pool_shared3 = pool::Exec::<hylic::domain::Shared>::from_pool(pool, pool_spec);
    let pool_local   = pool::Exec::<hylic::domain::Local>::from_pool(pool, pool_spec);
    let pool_local2  = pool::Exec::<hylic::domain::Local>::from_pool(pool, pool_spec);
    let pool_local3  = pool::Exec::<hylic::domain::Local>::from_pool(pool, pool_spec);
    let pool_owned   = pool::Exec::<hylic::domain::Owned>::from_pool(pool, pool_spec);

    let work = Arc::new(s.work.clone());

    vec![
        // ── hylic rayon (Shared only) ─────────────────
        BenchMode { name: id::RAYON_SHARED,
            run: Box::new(move || dom::RAYON.run(fold, treeish, root)) },

        // ── hylic pool (all domains) ──────────────────
        BenchMode { name: id::POOL_SHARED,
            run: Box::new(move || pool_shared.run(fold, treeish, root)) },
        BenchMode { name: id::POOL_LOCAL,
            run: Box::new(move || pool_local.run(&local_fold, &local_tree, root)) },
        BenchMode { name: id::POOL_OWNED,
            run: Box::new(move || pool_owned.run(&owned_fold, &owned_tree, root)) },

        // ── hylic ParLazy lift ──────────────────────────
        BenchMode { name: id::PARREF_FUSED_SHARED,
            run: Box::new(move || dom::FUSED.run_lifted(&par_lazy_fused, fold, treeish, root)) },
        BenchMode { name: id::PARREF_RAYON_SHARED,
            run: Box::new(move || dom::RAYON.run_lifted(&par_lazy_rayon, fold, treeish, root)) },
        BenchMode { name: id::PARREF_POOL_SHARED,
            run: Box::new(move || pool_shared2.run_lifted(&par_lazy_pool, fold, treeish, root)) },

        // ── hylic ParEager lift ─────────────────────────
        BenchMode { name: id::EAGER_FUSED_SHARED,
            run: Box::new(move || dom::FUSED.run_lifted(&par_eager_fused, fold, treeish, root)) },
        BenchMode { name: id::EAGER_RAYON_SHARED,
            run: Box::new(move || dom::RAYON.run_lifted(&par_eager_rayon, fold, treeish, root)) },
        BenchMode { name: id::EAGER_POOL_SHARED,
            run: Box::new(move || pool_shared3.run_lifted(&par_eager_pool, fold, treeish, root)) },

        // ── hylic lifts (Local domain) ────────────────
        BenchMode { name: id::PARREF_FUSED_LOCAL,
            run: Box::new(move || hylic::domain::local::FUSED.run_lifted(&par_lazy_fused_local, &local_fold_plf, &local_tree_plf, root)) },
        BenchMode { name: id::PARREF_POOL_LOCAL,
            run: Box::new(move || pool_local2.run_lifted(&par_lazy_pool_local, &local_fold_pll, &local_tree_pll, root)) },
        BenchMode { name: id::EAGER_FUSED_LOCAL,
            run: Box::new(move || hylic::domain::local::FUSED.run_lifted(&par_eager_fused_local, &local_fold_elf, &local_tree_elf, root)) },
        BenchMode { name: id::EAGER_POOL_LOCAL,
            run: Box::new(move || pool_local3.run_lifted(&par_eager_pool_local, &local_fold_ell, &local_tree_ell, root)) },

        // ── hylic funnel (scoped pool, created per call) ──
        BenchMode { name: id::FUNNEL_SHARED,
            run: {
                let nw = super::config::bench_workers();
                Box::new(move || {
                    funnel::Exec::<hylic::domain::Shared>::with(funnel::Spec::default(nw), |exec| {
                        exec.run(fold, treeish, root)
                    })
                })
            }},

        // ── handrolled parallel ───────────────────────
        BenchMode { name: id::HAND_RAYON,
            run: Box::new(|| handrolled_rayon(s)) },
        BenchMode { name: id::HAND_POOL,
            run: Box::new(move || handrolled_pool(&s.children, &work, pool, s.root)) },
        BenchMode { name: id::REAL_RAYON,
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
    // Same primitives as the Lift (view.join for binary-split parallelism),
    // just without FoldOps/TreeOps/SyncRef/domain abstractions.
    let view = PoolExecView::new(pool);

    fn recurse(
        children: &[Vec<NodeId>], work: &WorkSpec,
        view: &PoolExecView, node: NodeId,
    ) -> u64 {
        work.do_graph();
        let mut heap = work.do_init();
        let ch = &children[node];
        if ch.len() <= 1 {
            for &child in ch {
                work.do_accumulate(&mut heap, &recurse(children, work, view, child));
            }
        } else {
            let mid = ch.len() / 2;
            let (left, right) = view.join(
                || ch[..mid].iter().map(|&c| recurse(children, work, view, c)).collect::<Vec<_>>(),
                || ch[mid..].iter().map(|&c| recurse(children, work, view, c)).collect::<Vec<_>>(),
            );
            for r in left.iter().chain(right.iter()) {
                work.do_accumulate(&mut heap, r);
            }
        }
        work.do_finalize(&heap)
    }
    recurse(&children, &work, &view, root)
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
