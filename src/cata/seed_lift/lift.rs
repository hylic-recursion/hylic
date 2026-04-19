//! SeedLift: the FP core. Transforms treeish and fold into the
//! LiftedNode domain. Implements the bifunctor Lift trait.

use std::sync::Arc;
use crate::domain::shared;
use crate::graph::{self, Edgy, Treeish};
use super::types::{LiftedNode, LiftedHeap};

pub struct SeedLift<N, Seed> {
    pub(crate) grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
}

impl<N, Seed> Clone for SeedLift<N, Seed> {
    fn clone(&self) -> Self { SeedLift { grow: self.grow.clone() } }
}

impl<N: Clone + 'static, Seed: Clone + 'static> SeedLift<N, Seed> {
    pub fn new(grow: impl Fn(&Seed) -> N + Send + Sync + 'static) -> Self {
        SeedLift { grow: Arc::new(grow) }
    }

    // ANCHOR: lift_treeish
    pub fn lift_treeish(
        &self,
        t: Treeish<N>,
        entry_seeds: Edgy<(), Seed>,
    ) -> Treeish<LiftedNode<Seed, N>> {
        let grow = self.grow.clone();
        graph::treeish_visit(move |n: &LiftedNode<Seed, N>, cb: &mut dyn FnMut(&LiftedNode<Seed, N>)| {
            match n {
                LiftedNode::Node(node) => {
                    t.visit(node, &mut |child: &N| {
                        cb(&LiftedNode::Node(child.clone()));
                    });
                }
                LiftedNode::Seed(seed) => {
                    cb(&LiftedNode::Node(grow(seed)));
                }
                LiftedNode::Entry => {
                    entry_seeds.visit(&(), &mut |seed: &Seed| {
                        cb(&LiftedNode::Node(grow(seed)));
                    });
                }
            }
        })
    }
    // ANCHOR_END: lift_treeish

    pub fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self,
        f: shared::fold::Fold<N, H, R>,
        entry_heap_fn: impl Fn() -> H + Send + Sync + 'static,
    ) -> shared::fold::Fold<LiftedNode<Seed, N>, LiftedHeap<H, R>, R> {
        let f1 = f.clone();
        let f2 = f.clone();
        let f3 = f;
        shared::fold::fold(
            move |n: &LiftedNode<Seed, N>| -> LiftedHeap<H, R> {
                match n {
                    LiftedNode::Node(node) => LiftedHeap::Active(f1.init(node)),
                    LiftedNode::Seed(_) => LiftedHeap::Relay(None),
                    LiftedNode::Entry => LiftedHeap::Active(entry_heap_fn()),
                }
            },
            move |heap: &mut LiftedHeap<H, R>, result: &R| {
                match heap {
                    LiftedHeap::Active(h) => f2.accumulate(h, result),
                    LiftedHeap::Relay(slot) => *slot = Some(result.clone()),
                }
            },
            move |heap: &LiftedHeap<H, R>| -> R {
                match heap {
                    LiftedHeap::Active(h) => f3.finalize(h),
                    LiftedHeap::Relay(Some(r)) => r.clone(),
                    LiftedHeap::Relay(None) => panic!("relay finalized without child result"),
                }
            },
        )
    }
}
