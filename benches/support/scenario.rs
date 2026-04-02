use std::sync::Arc;
use hylic::domain::shared::{self as dom, Treeish, Executor};

use super::tree::{self, NodeId, TreeSpec};
use super::work::WorkSpec;

/// Complete benchmark scenario definition.
pub struct ScenarioDef {
    pub name: &'static str,
    pub moniker: &'static str,
    pub tree: TreeSpec,
    pub work: WorkSpec,
}

/// Ready-to-run scenario with pre-built tree and Shared-domain fold/treeish.
///
/// Domain variants (Local, Owned) are constructed in hylic_runners
/// directly from `work` + `children` — same single layer of indirection.
pub struct PreparedScenario {
    pub name: String,
    pub children: Arc<Vec<Vec<NodeId>>>,
    pub node_count: usize,
    pub work: WorkSpec,
    pub fold: dom::Fold<NodeId, u64, u64>,
    pub treeish: Treeish<NodeId>,
    pub root: NodeId,
    pub expected: u64,
}

/// Build a Shared-domain treeish from work + children.
pub fn make_shared_treeish(work: &WorkSpec, children: &Arc<Vec<Vec<NodeId>>>) -> Treeish<NodeId> {
    let w = work.clone();
    let ch = children.clone();
    dom::treeish_visit(move |n: &NodeId, cb: &mut dyn FnMut(&NodeId)| {
        w.do_graph();
        for &child in &ch[*n] { cb(&child); }
    })
}

/// Build a Shared-domain fold from work.
pub fn make_shared_fold(work: &WorkSpec) -> dom::Fold<NodeId, u64, u64> {
    let w1 = work.clone();
    let w2 = work.clone();
    let w3 = work.clone();
    dom::fold(
        move |_node: &NodeId| w1.do_init(),
        move |heap: &mut u64, child: &u64| w2.do_accumulate(heap, child),
        move |heap: &u64| w3.do_finalize(heap),
    )
}

impl PreparedScenario {
    pub fn from_def(def: &ScenarioDef, label: &str) -> Self {
        let (children, node_count) = tree::gen_tree(&def.tree);
        let treeish = make_shared_treeish(&def.work, &children);
        let fold = make_shared_fold(&def.work);
        let expected = dom::FUSED.run(&fold, &treeish, &0);

        PreparedScenario {
            name: format!("{}/{}", def.moniker, label),
            children,
            node_count,
            work: def.work.clone(),
            fold,
            treeish,
            root: 0,
            expected,
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
    pub fn from_env() -> Self {
        match std::env::var("HYLIC_BENCH_SCALE").as_deref() {
            Ok("large" | "Large" | "LARGE") => Scale::Large,
            _ => Scale::Small,
        }
    }
}
