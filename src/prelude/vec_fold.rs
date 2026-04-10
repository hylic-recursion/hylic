use crate::domain::shared::fold::{fold as fold_fn, Fold};

#[derive(Debug, Clone)]
pub struct VecHeap<N, R> {
    pub node: N,
    pub childresults: Vec<R>,
}

impl<N, R> VecHeap<N, R> {
    pub fn new(node: N) -> Self { VecHeap { node, childresults: Vec::new() } }
    pub fn push(&mut self, result: R) { self.childresults.push(result); }
}

pub fn vec_fold<N, R>(
    finalize: impl Fn(&VecHeap<N, R>) -> R + Send + Sync + 'static,
) -> Fold<N, VecHeap<N, R>, R>
where N: Clone + 'static, R: Clone + 'static,
{
    fold_fn(
        |node: &N| VecHeap::new(node.clone()),
        |heap: &mut VecHeap<N, R>, result: &R| heap.push(result.clone()),
        finalize,
    )
}

pub type VecFold<N, R> = Fold<N, VecHeap<N, R>, R>;
