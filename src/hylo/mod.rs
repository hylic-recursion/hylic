pub mod graph_with_fold;

pub use graph_with_fold::GraphWithFold;

pub type HeapOfTopFn<Top, HeapT> = Box<dyn Fn(&Top) -> HeapT + Send + Sync>;
