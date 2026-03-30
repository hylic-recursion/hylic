pub mod seed_adapter;
pub mod graph_with_fold;

pub use seed_adapter::SeedFoldAdapter;
pub use graph_with_fold::GraphWithFold;

pub type HeapOfTopFn<Top, HeapT> = Box<dyn Fn(&Top) -> HeapT + Send + Sync>;
