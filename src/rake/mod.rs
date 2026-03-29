pub mod transformations;
pub mod explainer;
pub mod core;
pub mod helper;
pub mod vec_compress;
pub mod par;
pub mod execute;

pub use core::RakeCompress;

pub type Rake<N, H> = RakeCompress<N, H, H>;

pub fn rake_compress<N, H, R>(
    rake_null: impl Fn(&N) -> H + Send + Sync + 'static,
    rake_add: impl Fn(&mut H, &R) + Send + Sync + 'static,
    compress: impl Fn(&H) -> R + Send + Sync + 'static,
) -> RakeCompress<N, H, R> where N: 'static {
    RakeCompress::new(rake_null, rake_add, compress)
}

pub fn rake<N, H>(
    rake_null: impl Fn(&N) -> H + Send + Sync + 'static,
    rake_add: impl Fn(&mut H, &H) + Send + Sync + 'static,
) -> RakeCompress<N, H, H> where N: 'static, H: Clone + 'static {
    RakeCompress::new(rake_null, rake_add, |heap| heap.clone())
}

pub use vec_compress::{vec_compress, VecHeapCompress, VecHeap};
