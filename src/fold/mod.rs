pub mod algebra;
pub(crate) mod combinators;

pub use algebra::Fold;
pub use crate::ops::FoldOps;

pub type InitFn<N, H> = Box<dyn Fn(&N) -> H + Send + Sync>;
pub type AccumulateFn<H, R> = Box<dyn Fn(&mut H, &R) + Send + Sync>;
pub type FinalizeFn<H, R> = Box<dyn Fn(&H) -> R + Send + Sync>;

impl<N: 'static, H: 'static, R: 'static> FoldOps<N, H, R> for Fold<N, H, R> {
    fn init(&self, node: &N) -> H { Fold::init(self, node) }
    fn accumulate(&self, heap: &mut H, result: &R) { Fold::accumulate(self, heap, result) }
    fn finalize(&self, heap: &H) -> R { Fold::finalize(self, heap) }
}

pub fn fold<N, H, R>(
    init: impl Fn(&N) -> H + Send + Sync + 'static,
    accumulate: impl Fn(&mut H, &R) + Send + Sync + 'static,
    finalize: impl Fn(&H) -> R + Send + Sync + 'static,
) -> Fold<N, H, R> where N: 'static, H: 'static, R: 'static {
    Fold::new(init, accumulate, finalize)
}

pub fn simple_fold<N, H>(
    init: impl Fn(&N) -> H + Send + Sync + 'static,
    accumulate: impl Fn(&mut H, &H) + Send + Sync + 'static,
) -> Fold<N, H, H> where N: 'static, H: Clone + 'static {
    Fold::new(init, accumulate, |heap| heap.clone())
}
