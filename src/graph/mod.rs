use std::sync::Arc;

use either::Either;

pub mod types;
pub mod traced;

pub mod graph;
pub mod graph_with_seed_and_err;

pub type ContramapFunc<NodeV, NodeE> = dyn Fn(&Either<NodeE, NodeV>) -> Either<Vec<Either<NodeE, NodeV>>, NodeV> + Send + Sync;
pub type OptContramapFunc<NodeV, NodeE> = Option<Box<ContramapFunc<NodeV, NodeE>>>;
pub type OptContramapFuncRc<NodeV, NodeE> = Option<Arc<ContramapFunc<NodeV, NodeE>>>;

pub mod treeish_from_err_edgy;
pub mod treeish_from_deperr;
pub mod edgy_from_deperr;

pub mod raco_adapter;

pub use types::{Treeish, Edgy, treeish, treeish_visit, edgy, edgy_visit};
pub use graph::Graph;
pub use graph::GraphWithRaco;

pub use graph_with_seed_and_err::GraphWithSeedAndErr;
pub use graph_with_seed_and_err::GraphWithSeedAndErrRaco;

pub use raco_adapter::RacoAdapterSeedErr;
pub use raco_adapter::core::RacoAdapter;

pub use treeish_from_err_edgy::TreeishFromErrEdgy;
pub use edgy_from_deperr::EdgyFromDepErr;
pub use treeish_from_deperr::TreeishFromDepErr;

// Existing components
pub use traced::{Traced, traced_treeish};
