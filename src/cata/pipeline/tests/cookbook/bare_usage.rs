//! Cookbook: LiftBare — apply lifts directly to (treeish, fold) with
//! no pipeline machinery.

use crate::cata::exec::funnel;
use crate::domain::shared::{self as dom, fold::fold};
use crate::domain::Shared;
use crate::graph::{treeish, Treeish};
use crate::ops::lift::bare::LiftBare;

#[derive(Clone, Debug)]
struct Node { val: u64, children: Vec<Node> }

#[test]
fn apply_bare_returns_transformed_pair() {
    let base_treeish: Treeish<Node> = treeish(|n: &Node| n.children.clone());
    let base_fold = fold(
        |n: &Node| n.val,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let lift = Shared::wrap_init_lift::<Node, u64, u64, _>(
        |n: &Node, orig: &dyn Fn(&Node) -> u64| orig(n) + 1,
    );
    let (t2, f2) = lift.apply_bare(base_treeish, base_fold);

    // Run the transformed pair via the Funnel executor.
    let root = Node { val: 3,
        children: vec![Node { val: 1, children: vec![] }] };
    let r = dom::exec(funnel::Spec::default(4)).run(&f2, &t2, &root);
    // wrap_init adds +1: child = 1+1 = 2; root = 3+1 + 2 = 6.
    assert_eq!(r, 6);
}

#[test]
fn run_on_one_shot() {
    let base_treeish: Treeish<Node> = treeish(|n: &Node| n.children.clone());
    let base_fold = fold(
        |n: &Node| n.val,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let lift = Shared::zipmap_lift::<Node, u64, u64, bool, _>(|r: &u64| *r > 4);

    let root = Node { val: 3,
        children: vec![Node { val: 1, children: vec![] }] };
    let r: (u64, bool) = lift.run_on(
        &dom::exec(funnel::Spec::default(4)),
        base_treeish,
        base_fold,
        &root,
    );
    // base sum = 3 + 1 = 4; zipmap: false (not > 4).
    assert_eq!(r, (4u64, false));
}
