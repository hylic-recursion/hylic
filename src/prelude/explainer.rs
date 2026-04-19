//! Explainer — computation tracing as a Lift. Polymorphic in (N, Seed, H, R).

use std::sync::Arc;
use crate::graph::{treeish, Treeish, Edgy};
use crate::domain::shared::fold::{self as sfold, Fold};
use crate::ops::Lift;

#[derive(Clone)]
pub struct ExplainerStep<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub incoming_result: ExplainerResult<N, H, R>,
    pub resulting_heap: H,
}

#[derive(Clone)]
pub struct ExplainerHeap<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub initial_heap: H,
    pub node: N,
    pub transitions: Vec<ExplainerStep<N, H, R>>,
    pub working_heap: H,
}

impl<N: Clone, H: Clone, R: Clone> ExplainerHeap<N, H, R> {
    pub fn new(node: N, heap: H) -> Self {
        ExplainerHeap {
            initial_heap: heap.clone(),
            node,
            transitions: Vec::new(),
            working_heap: heap,
        }
    }
}

#[derive(Clone)]
pub struct ExplainerResult<N, H, R>
where N: Clone, H: Clone, R: Clone,
{
    pub orig_result: R,
    pub heap: ExplainerHeap<N, H, R>,
}

#[derive(Clone, Copy)]
pub struct Explainer;

impl<N, Seed, H, R> Lift<N, Seed, H, R> for Explainer
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    type N2 = N;
    type Seed2 = Seed;
    type MapH = ExplainerHeap<N, H, R>;
    type MapR = ExplainerResult<N, H, R>;

    fn apply<T>(
        &self,
        grow: Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        seeds: Edgy<N, Seed>,
        treeish_in: Treeish<N>,
        fold: Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N + Send + Sync>,
            Edgy<N, Seed>,
            Treeish<N>,
            Fold<N, ExplainerHeap<N, H, R>, ExplainerResult<N, H, R>>,
        ) -> T,
    ) -> T {
        let f1 = fold.clone();
        let f2 = fold.clone();
        let f3 = fold;
        let wrapped = sfold::fold(
            move |n: &N| ExplainerHeap::new(n.clone(), f1.init(n)),
            move |heap: &mut ExplainerHeap<N, H, R>, result: &ExplainerResult<N, H, R>| {
                f2.accumulate(&mut heap.working_heap, &result.orig_result);
                heap.transitions.push(ExplainerStep {
                    incoming_result: result.clone(),
                    resulting_heap: heap.working_heap.clone(),
                });
            },
            move |heap: &ExplainerHeap<N, H, R>| ExplainerResult {
                orig_result: f3.finalize(&heap.working_heap),
                heap: heap.clone(),
            },
        );
        cont(grow, seeds, treeish_in, wrapped)
    }

    fn lift_root(&self, root: &N) -> N { root.clone() }
}

pub fn treeish_for_explres<N: Clone + 'static, H: Clone + 'static, R: Clone + 'static>(
) -> Treeish<ExplainerResult<N, H, R>> {
    treeish(|x: &ExplainerResult<N, H, R>| {
        x.heap.transitions.iter().map(|step| step.incoming_result.clone()).collect()
    })
}
