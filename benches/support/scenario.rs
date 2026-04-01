use std::sync::Arc;
use hylic::graph::{treeish_visit, Treeish};
use hylic::fold::{self, Fold};
use hylic::cata::exec::{self, Executor};

use super::tree::{self, NodeId, TreeSpec};
use super::work::WorkSpec;

/// Complete benchmark scenario definition.
pub struct ScenarioDef {
    pub name: &'static str,
    pub moniker: &'static str,
    pub tree: TreeSpec,
    pub work: WorkSpec,
}

/// Ready-to-run scenario with pre-built tree, fold, treeish, AND raw closures.
pub struct PreparedScenario {
    pub name: String,
    pub children: Arc<Vec<Vec<NodeId>>>,
    pub node_count: usize,
    pub work: WorkSpec,
    pub fold: Fold<NodeId, u64, u64>,
    pub treeish: Treeish<NodeId>,
    pub root: NodeId,
    pub expected: u64,
    // Raw closures for domain-variant construction
    pub init_fn: Arc<dyn Fn(&NodeId) -> u64 + Send + Sync>,
    pub acc_fn: Arc<dyn Fn(&mut u64, &u64) + Send + Sync>,
    pub fin_fn: Arc<dyn Fn(&u64) -> u64 + Send + Sync>,
    pub graph_fn: Arc<dyn Fn(&NodeId, &mut dyn FnMut(&NodeId)) + Send + Sync>,
}

impl PreparedScenario {
    pub fn from_def(def: &ScenarioDef, label: &str) -> Self {
        let (children, node_count) = tree::gen_tree(&def.tree);
        let ch = children.clone();
        let w = def.work.clone();
        let w2 = def.work.clone();

        let graph_fn: Arc<dyn Fn(&NodeId, &mut dyn FnMut(&NodeId)) + Send + Sync> = {
            let ch = ch.clone();
            let w = w.clone();
            Arc::new(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
                w.do_graph();
                for &child in &ch[*n] { cb(&child); }
            })
        };

        let init_fn: Arc<dyn Fn(&NodeId) -> u64 + Send + Sync> = {
            let w = w2.clone();
            Arc::new(move |_node: &NodeId| -> u64 { w.do_init() })
        };
        let acc_fn: Arc<dyn Fn(&mut u64, &u64) + Send + Sync> = {
            let w = def.work.clone();
            Arc::new(move |heap: &mut u64, child: &u64| { w.do_accumulate(heap, child); })
        };
        let fin_fn: Arc<dyn Fn(&u64) -> u64 + Send + Sync> = {
            let w = def.work.clone();
            Arc::new(move |heap: &u64| -> u64 { w.do_finalize(heap) })
        };

        let treeish = {
            let gf = graph_fn.clone();
            treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| { gf(n, cb) })
        };

        let fold = {
            let i = init_fn.clone();
            let a = acc_fn.clone();
            let f = fin_fn.clone();
            fold::fold(
                move |n: &NodeId| i(n),
                move |h: &mut u64, c: &u64| a(h, c),
                move |h: &u64| f(h),
            )
        };

        // Correctness baseline — run fused once to get expected result
        let expected = exec::FUSED.run(&fold, &treeish, &0);

        PreparedScenario {
            name: format!("{}/{}", def.moniker, label),
            children,
            node_count,
            work: def.work.clone(),
            fold,
            treeish,
            root: 0,
            expected,
            init_fn,
            acc_fn,
            fin_fn,
            graph_fn,
        }
    }
}

// ── Scenario catalog ───────────────────────────────────────

fn def(name: &'static str, moniker: &'static str, tree: TreeSpec, work: WorkSpec) -> ScenarioDef {
    ScenarioDef { name, moniker, tree, work }
}

fn w(init: u64, acc: u64, fin: u64, graph: u64, io: u64) -> WorkSpec {
    WorkSpec { init_work: init, accumulate_work: acc, finalize_work: fin, graph_work: graph, graph_io_us: io }
}

pub fn all_scenarios(scale: Scale) -> Vec<ScenarioDef> {
    let (n, n_large) = match scale {
        Scale::Small => (200, 500),
        Scale::Large => (2000, 5000),
    };
    vec![
        def("noop",         "noop",     TreeSpec { node_count: n, branch_factor: 8 },  w(0, 0, 0, 0, 0)),
        def("hashtable",    "hash",     TreeSpec { node_count: n, branch_factor: 8 },  w(5_000, 1_000, 0, 5_000, 0)),
        def("parse-light",  "parse-lt", TreeSpec { node_count: n, branch_factor: 8 },  w(50_000, 5_000, 5_000, 10_000, 0)),
        def("parse-heavy",  "parse-hv", TreeSpec { node_count: n, branch_factor: 8 },  w(200_000, 10_000, 10_000, 50_000, 0)),
        def("aggregate",    "aggr",     TreeSpec { node_count: n, branch_factor: 8 },  w(5_000, 100_000, 5_000, 5_000, 0)),
        def("transform",    "xform",    TreeSpec { node_count: n, branch_factor: 8 },  w(5_000, 5_000, 100_000, 5_000, 0)),
        def("finalize-only","fin",      TreeSpec { node_count: n, branch_factor: 8 },  w(0, 0, 100_000, 0, 0)),
        def("balanced",     "bal",      TreeSpec { node_count: n, branch_factor: 8 },  w(50_000, 50_000, 50_000, 50_000, 0)),
        def("io-bound",     "io",       TreeSpec { node_count: n, branch_factor: 8 },  w(5_000, 0, 0, 0, 200)),
        def("wide-shallow", "wide",     TreeSpec { node_count: n, branch_factor: 20 }, w(50_000, 10_000, 10_000, 10_000, 0)),
        def("deep-narrow",  "deep",     TreeSpec { node_count: n, branch_factor: 2 },  w(50_000, 10_000, 10_000, 10_000, 0)),
        def("large-dense",  "lg-dense", TreeSpec { node_count: n_large, branch_factor: 10 }, w(50_000, 10_000, 10_000, 10_000, 0)),
    ]
}

#[derive(Clone, Copy)]
pub enum Scale { Small, Large }

impl Scale {
    /// Read from HYLIC_BENCH_SCALE env var. Defaults to Small.
    pub fn from_env() -> Self {
        match std::env::var("HYLIC_BENCH_SCALE").as_deref() {
            Ok("large" | "Large" | "LARGE") => Scale::Large,
            _ => Scale::Small,
        }
    }
}
