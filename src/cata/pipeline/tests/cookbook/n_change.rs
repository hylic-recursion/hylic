//! Cookbook: N-change lifts.
//!
//! `n_lift` for context-dependent N-change (depth annotation);
//! `map_n_bi_lift` (as `map_node_bi` method) for bijective
//! N-wrap.

use std::sync::Arc;
use crate::cata::pipeline::{TreeishPipeline, PipelineExec, LiftedSugarsShared};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::{treeish, treeish_visit, Treeish};
use crate::domain::Shared;

#[derive(Clone, Debug)]
struct Node { val: u64, children: Vec<Node> }

#[derive(Clone, Debug)]
struct WithDepth { node: Node, depth: u32 }

#[test]
fn contramap_node_arc_wraps_non_clone_payload() {
    // Demonstrate bijective N-wrap with Arc.
    let tree = Node {
        val: 10,
        children: vec![
            Node { val: 1, children: vec![] },
            Node { val: 2, children: vec![] },
        ],
    };
    let base_treeish: Treeish<Node> = treeish(|n: &Node| n.children.clone());
    let base_fold = fold(
        |n: &Node| n.val,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let r: u64 = TreeishPipeline::new(base_treeish, &base_fold)
        .lift()
        .map_node_bi(
            |n: &Node| Arc::new(n.clone()),
            |a: &Arc<Node>| (**a).clone(),
        )
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &Arc::new(tree));
    assert_eq!(r, 13);
}

#[test]
fn inline_lift_depth_annotates() {
    let tree = Node {
        val: 100,
        children: vec![
            Node {
                val: 10,
                children: vec![
                    Node { val: 1, children: vec![] },
                    Node { val: 2, children: vec![] },
                ],
            },
            Node { val: 20, children: vec![] },
        ],
    };

    let depth_weighted_fold = fold(
        |n: &WithDepth| n.node.val * (n.depth as u64),
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let base_treeish: Treeish<Node> = treeish(|n: &Node| n.children.clone());

    let lift = Shared::n_lift::<Node, u64, u64, WithDepth, _, _, _>(
        |n: &Node| WithDepth { node: n.clone(), depth: 0 },
        |base: &Treeish<Node>| -> Treeish<WithDepth> {
            let base = base.clone();
            treeish_visit(move |wd: &WithDepth, cb: &mut dyn FnMut(&WithDepth)| {
                let d = wd.depth;
                base.visit(&wd.node, &mut |child: &Node| {
                    cb(&WithDepth { node: child.clone(), depth: d + 1 })
                });
            })
        },
        |wd: &WithDepth| wd.node.clone(),
    );

    // Build TreeishPipeline over the ANNOTATED WithDepth treeish so
    // the fold's N matches. The n_lift here is Stage-2, but here
    // we illustrate a simpler approach: pre-annotate via n_lift
    // against a base treeish-over-Node pipeline. Fold sees WithDepth.
    let base_pipeline = TreeishPipeline::<Shared, Node, u64, u64>::new(
        base_treeish,
        &fold(
            |n: &Node| n.val,
            |h: &mut u64, c: &u64| *h += c,
            |h: &u64| *h,
        ),
    );
    // Override fold via map_node_bi-like trick: we use n_lift
    // which changes N to WithDepth; the downstream fold must accept
    // WithDepth. Applying the provided depth-weighted fold requires
    // matching; we use map_treeish-free approach: compose n_lift
    // with a full re-fold via wrap_finalize returning weighted sum.
    let _ = depth_weighted_fold; // depth-weighted variant shown in n_lift.rs
    let r: u64 = base_pipeline
        .lift()
        .then_lift(lift)
        // After lift, N2 = WithDepth, fold is contramapped to take
        // WithDepth (returning n.val). We post-process via wrap_init
        // to emit val * depth instead.
        .wrap_init(|wd: &WithDepth, _orig: &dyn Fn(&WithDepth) -> u64| wd.node.val * (wd.depth as u64))
        .run_from_node(
            &dom::exec(funnel::Spec::default(4)),
            &WithDepth { node: tree.clone(), depth: 0 },
        );

    // root 100*0 + level-1 (10*1 + 20*1) + level-2 (1*2 + 2*2) = 0 + 30 + 6 = 36
    assert_eq!(r, 36);
}
