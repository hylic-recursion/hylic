// formulate rake_compress as:
// - a heap that just has the node and a vector of sub-results
// - the compress function
//   - the rake_null is the heap with the empty vector
//   - the rake_add adds the result to the vector

use super::{rake_compress, RakeCompress};

#[derive(Debug, Clone)]
pub struct VecHeap<N, R> {
    pub node: N,
    pub childresults: Vec<R>,
}

impl<N, R> VecHeap<N, R> {
    pub fn new(node: N) -> Self {
        VecHeap {
            node,
            childresults: Vec::new(),
        }
    }

    pub fn rake_null(node: N) -> Self {
        VecHeap {
            node,
            childresults: Vec::new(),
        }
    }

    pub fn rake_add(&mut self, result: R) {
        self.childresults.push(result);
    }

}

// forwards to rake_compress
pub fn vec_compress<N, R>(
    compress: impl Fn(&VecHeap<N, R>) -> R + Send + Sync + 'static,
) -> RakeCompress<N, VecHeap<N, R>, R> where
N: Clone + 'static,
R: Clone + 'static,
{
    rake_compress(
        |node: &N| VecHeap::rake_null(node.clone()),
        |heap: &mut VecHeap<N, R>, result: &R| {
            heap.rake_add(result.clone());
        },
        compress,
    )
}

pub type VecHeapCompress<N, R> = RakeCompress<N, VecHeap<N, R>, R>;

