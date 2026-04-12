//! Explorative: type-morphing pre-lift on SeedPipeline.
//!
//! Tests that a pre-lift which changes the node type (Nt ≠ N)
//! works end-to-end through the pipeline. The SeedLift must
//! operate on Nt, not N. Grow must compose through lift_root.
//!
//! This test uses the production types (not self-contained) to
//! verify the actual pipeline wiring.

use std::sync::Arc;
use crate::domain::shared::{self as dom, fold};
use crate::graph;
use crate::ops::{LiftOps, OuterLift, IdentityLift};
use crate::cata::seed_lift::pipeline::SeedPipeline;

// A pre-lift that tags each node with a string.
// N = usize → Nt = (usize, &'static str)
// H and R pass through.
struct TagPreLift(&'static str);

impl Clone for TagPreLift {
    fn clone(&self) -> Self { TagPreLift(self.0) }
}

impl<R: Clone + 'static> LiftOps<usize, R, (usize, &'static str)> for TagPreLift {
    type LiftedH<H: Clone + 'static> = H;
    type LiftedR<H: Clone + 'static> = R;

    fn lift_treeish(&self, t: graph::Treeish<usize>) -> graph::Treeish<(usize, &'static str)> {
        let tag = self.0;
        t.treemap(move |n: &usize| (*n, tag), |pair: &(usize, &str)| pair.0)
    }

    fn lift_fold<H: Clone + 'static>(
        &self, f: dom::Fold<usize, H, R>,
    ) -> dom::Fold<(usize, &'static str), H, R> {
        f.contramap(|pair: &(usize, &'static str)| pair.0)
    }

    fn lift_root(&self, root: &usize) -> (usize, &'static str) {
        (*root, self.0)
    }
}

impl<Inner, N, R> OuterLift<Inner, N, R, usize, (usize, &'static str)> for TagPreLift
where
    N: 'static,
    R: Clone + 'static,
    Inner: LiftOps<N, R, usize>,
{
    type LiftedH<H: Clone + 'static> = Inner::LiftedH<H>;
    type LiftedR<H: Clone + 'static> = Inner::LiftedR<H>;

    fn lift_treeish(&self, t: graph::Treeish<usize>) -> graph::Treeish<(usize, &'static str)> {
        let tag = self.0;
        t.treemap(move |n: &usize| (*n, tag), |pair: &(usize, &str)| pair.0)
    }

    fn lift_fold<H: Clone + 'static>(
        &self,
        f: dom::Fold<usize, Inner::LiftedH<H>, Inner::LiftedR<H>>,
    ) -> dom::Fold<(usize, &'static str), Inner::LiftedH<H>, Inner::LiftedR<H>> {
        f.contramap(|pair: &(usize, &'static str)| pair.0)
    }

    fn lift_root(&self, root: &usize) -> (usize, &'static str) {
        (*root, self.0)
    }
}

#[test]
fn type_morphing_prelift_changes_node_type() {
    // Pipeline: N=usize, Seed=usize
    let ch = vec![vec![1usize, 2], vec![3], vec![], vec![]];
    let seeds = graph::edgy_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &c in &ch[*n] { cb(&c); }
    });
    let f = fold::fold(|n: &usize| *n as u64, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let pipeline = SeedPipeline::new(|s: &usize| *s, seeds, &f);

    // Apply type-morphing pre-lift: N=usize → Nt=(usize, &str)
    // After this, the SeedLift operates on (usize, &str) nodes.
    // Grow produces (usize, &str) via lift_root.
    // The executor sees LiftedNode<usize, (usize, &str)>.
    let tagged = pipeline.apply_pre_lift(TagPreLift("tagged"));

    // The result type should be the same (R=u64, transparent in R).
    // The computation is the same — contramap extracts .0.
    let result = tagged.run_from_slice(&dom::FUSED, &[0], 0u64);
    assert_eq!(result, 6); // 0+1+2+3
}
