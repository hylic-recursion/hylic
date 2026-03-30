pub mod algebra;
pub mod transformations;

pub use algebra::Fold;

pub type SimpleFold<N, H> = Fold<N, H, H>;

pub fn fold<N, H, R>(
    init: impl Fn(&N) -> H + Send + Sync + 'static,
    accumulate: impl Fn(&mut H, &R) + Send + Sync + 'static,
    finalize: impl Fn(&H) -> R + Send + Sync + 'static,
) -> Fold<N, H, R> where N: 'static {
    Fold::new(init, accumulate, finalize)
}

pub fn simple_fold<N, H>(
    init: impl Fn(&N) -> H + Send + Sync + 'static,
    accumulate: impl Fn(&mut H, &H) + Send + Sync + 'static,
) -> Fold<N, H, H> where N: 'static, H: Clone + 'static {
    Fold::new(init, accumulate, |heap| heap.clone())
}

// Convenience re-exports from prelude
pub use crate::prelude::vec_fold::{vec_fold, VecFold, VecHeap};
pub use crate::prelude::explainer;
pub use crate::prelude::format;
