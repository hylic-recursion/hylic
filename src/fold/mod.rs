pub mod algebra;

pub use algebra::Fold;

pub type InitFn<N, H> = Box<dyn Fn(&N) -> H + Send + Sync>;
pub type AccumulateFn<H, R> = Box<dyn Fn(&mut H, &R) + Send + Sync>;
pub type FinalizeFn<H, R> = Box<dyn Fn(&H) -> R + Send + Sync>;

/// The fold operations — init, accumulate, finalize.
///
/// `Fold<N, H, R>` implements this. So can any user-defined struct
/// for zero-boxing, fully-monomorphized execution.
pub trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}

impl<N: 'static, H: 'static, R: 'static> FoldOps<N, H, R> for Fold<N, H, R> {
    fn init(&self, node: &N) -> H { self.init(node) }
    fn accumulate(&self, heap: &mut H, result: &R) { self.accumulate(heap, result) }
    fn finalize(&self, heap: &H) -> R { self.finalize(heap) }
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
