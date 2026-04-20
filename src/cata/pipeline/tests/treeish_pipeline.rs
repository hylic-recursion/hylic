//! TreeishPipeline — the honest-base typestate for users who have
//! a Treeish<N> directly.

use crate::cata::pipeline::{TreeishPipeline, PipelineExec};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::treeish;

#[derive(Clone)]
struct N { val: u64, children: Vec<N> }

fn tree_fixture() -> N {
    N { val: 1, children: vec![
        N { val: 2, children: vec![
            N { val: 4, children: vec![] },
        ]},
        N { val: 3, children: vec![] },
    ]}
}

#[test]
fn run_from_node_on_bare_treeish() {
    // The ceremony-free path: a user has a Treeish<N> and runs it.
    let tree_graph = treeish(|n: &N| n.children.clone());
    let base_fold = fold(
        |n: &N| n.val,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let pipeline = TreeishPipeline::new(tree_graph, &base_fold);
    let r = pipeline.run_from_node(&dom::exec(funnel::Spec::default(4)), &tree_fixture());
    // 4 + 2 + 3 + 1 = 10.
    assert_eq!(r, 10);
}

#[test]
fn reshape_preserves_structure() {
    // reshape identity: no-op on both slots.
    let tree_graph = treeish(|n: &N| n.children.clone());
    let base_fold = fold(
        |n: &N| n.val,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let r = TreeishPipeline::new(tree_graph, &base_fold)
        .reshape::<N, u64, u64, _, _>(|t| t, |f| f)
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &tree_fixture());
    assert_eq!(r, 10);
}

#[test]
fn lift_and_zipmap() {
    // Lift into Stage 2, add a zipmap.
    let tree_graph = treeish(|n: &N| n.children.clone());
    let base_fold = fold(
        |n: &N| n.val,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let r: (u64, bool) = TreeishPipeline::new(tree_graph, &base_fold)
        .lift()
        .zipmap(|r: &u64| *r > 5)
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &tree_fixture());
    assert_eq!(r, (10, true));
}
