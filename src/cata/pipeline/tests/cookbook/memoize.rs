//! Cookbook: memoize_by on a diamond-shaped DAG.
//!
//! Without memoization the shared subtree is visited twice. With
//! memoize_by keyed on node id, the second visit replays cached
//! children.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::cata::pipeline::{TreeishPipeline, PipelineExec, LiftedSugarsShared};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::treeish;
use crate::domain::Shared;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct NodeId(u32);

fn diamond_registry() -> Arc<HashMap<NodeId, Vec<NodeId>>> {
    //     0
    //    / \
    //   1   2
    //    \ /
    //     3
    //     |
    //     4
    let mut m: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    m.insert(NodeId(0), vec![NodeId(1), NodeId(2)]);
    m.insert(NodeId(1), vec![NodeId(3)]);
    m.insert(NodeId(2), vec![NodeId(3)]);
    m.insert(NodeId(3), vec![NodeId(4)]);
    m.insert(NodeId(4), vec![]);
    Arc::new(m)
}

fn pipeline_with_visit_counter(
    counter: Arc<Mutex<u32>>,
) -> TreeishPipeline<Shared, NodeId, u32, u32> {
    let reg = diamond_registry();
    let t = treeish(move |n: &NodeId| {
        *counter.lock().unwrap() += 1;
        reg.get(n).cloned().unwrap_or_default()
    });
    let f = fold(
        |_n: &NodeId| 1u32,
        |h: &mut u32, c: &u32| *h += *c,
        |h: &u32| *h,
    );
    TreeishPipeline::new(t, &f)
}

#[test]
fn memoize_by_dedupes_shared_subtree_visits() {
    let root = NodeId(0);

    let visits_naive: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let _r_naive: u32 = pipeline_with_visit_counter(visits_naive.clone())
        .lift()
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &root);
    let naive_visits = *visits_naive.lock().unwrap();

    let visits_memo: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let _r_memo: u32 = pipeline_with_visit_counter(visits_memo.clone())
        .lift()
        .memoize_by(|n: &NodeId| n.0)
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &root);
    let memo_visits = *visits_memo.lock().unwrap();

    // Memoized path queries the base treeish fewer times for the
    // shared subtree (nodes 3 and 4 each visited once regardless of
    // how many ancestors reach them).
    assert!(
        memo_visits < naive_visits,
        "memoization reduced base visits (naive={naive_visits}, memo={memo_visits})"
    );
}
