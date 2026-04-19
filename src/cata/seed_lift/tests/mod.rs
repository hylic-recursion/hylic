mod core;
mod transforms;
mod entry;
pub mod explorative;

use crate::domain::shared::{self as dom, fold};
use crate::graph;
use super::types::LiftedNode;
use super::pipeline::{SeedPipeline, SeedPipelineExec};

type N = usize;
type S = usize;

fn test_children() -> Vec<Vec<usize>> {
    vec![vec![1, 2], vec![3], vec![], vec![]]
}

fn test_seeds_from_node() -> graph::Edgy<N, S> {
    let ch = test_children();
    graph::edgy_visit(move |n: &N, cb: &mut dyn FnMut(&S)| {
        for &c in &ch[*n] { cb(&c); }
    })
}

fn test_treeish() -> graph::Treeish<N> {
    let ch = test_children();
    graph::treeish_visit(move |n: &N, cb: &mut dyn FnMut(&N)| {
        for &c in &ch[*n] { cb(&c); }
    })
}

fn sum_fold() -> fold::Fold<N, u64, u64> {
    fold::fold(
        |n: &N| *n as u64,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    )
}

fn make_pipeline() -> SeedPipeline<N, S, u64, u64> {
    SeedPipeline::new(|seed: &S| *seed, test_seeds_from_node(), &sum_fold())
}
