pub mod adapter;
pub mod seed_adapter;
pub mod graph_with_fold;
pub mod seed_graph_fold;

pub use adapter::FoldAdapter;
pub use seed_adapter::SeedFoldAdapter;
pub use graph_with_fold::GraphWithFold;
pub use seed_graph_fold::SeedGraphFold;

pub type HeapOfTopFn<Top, HeapT> = Box<dyn Fn(&Top) -> HeapT + Send + Sync>;
