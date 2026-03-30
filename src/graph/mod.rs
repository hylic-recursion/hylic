use std::sync::Arc;
use either::Either;

pub mod types;
pub mod graph;

pub type ContramapFunc<NodeV, NodeE> = dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync;
pub type OptContramapFunc<NodeV, NodeE> = Option<Box<ContramapFunc<NodeV, NodeE>>>;
pub type OptContramapFuncRc<NodeV, NodeE> = Option<Arc<ContramapFunc<NodeV, NodeE>>>;

pub use types::{Treeish, Edgy, treeish, treeish_visit, edgy, edgy_visit};
pub use graph::Graph;

// Convenience re-export from prelude
pub use crate::prelude::traced::{Traced, traced_treeish};
