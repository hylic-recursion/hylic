//! Explorative: type-morphing pre-lift on SeedPipeline.
//!
//! Tests that a pre-lift which changes the node type (Nt ≠ N)
//! works end-to-end through the pipeline.

use crate::domain::shared::{self as dom, fold};
use crate::graph;
use crate::ops::Lift;
use crate::cata::seed_lift::{SeedPipeline, SeedPipelineExec};

struct TagPreLift(&'static str);

impl Clone for TagPreLift {
    fn clone(&self) -> Self { TagPreLift(self.0) }
}

impl Lift<usize, (usize, &'static str)> for TagPreLift {
    type MapH<H: Clone + 'static, R: Clone + 'static> = H;
    type MapR<H: Clone + 'static, R: Clone + 'static> = R;

    fn lift_treeish(&self, t: graph::Treeish<usize>) -> graph::Treeish<(usize, &'static str)> {
        let tag = self.0;
        t.treemap(move |n: &usize| (*n, tag), |pair: &(usize, &str)| pair.0)
    }

    fn lift_fold<H: Clone + 'static, R: Clone + 'static>(
        &self, f: dom::Fold<usize, H, R>,
    ) -> dom::Fold<(usize, &'static str), H, R> {
        f.contramap(|pair: &(usize, &'static str)| pair.0)
    }

    fn lift_root(&self, root: &usize) -> (usize, &'static str) {
        (*root, self.0)
    }
}

#[test]
fn type_morphing_prelift_changes_node_type() {
    let ch = vec![vec![1usize, 2], vec![3], vec![], vec![]];
    let seeds = graph::edgy_visit(move |n: &usize, cb: &mut dyn FnMut(&usize)| {
        for &c in &ch[*n] { cb(&c); }
    });
    let f = fold::fold(|n: &usize| *n as u64, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let pipeline = SeedPipeline::new(|s: &usize| *s, seeds, &f);

    let tagged = pipeline.apply_pre_lift(TagPreLift("tagged"));
    let result = tagged.run_from_slice(&dom::FUSED, &[0], 0u64);
    assert_eq!(result, 6);
}
