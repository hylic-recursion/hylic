pub mod pool;
pub mod lazy;
pub mod eager;

pub use pool::{WorkPool, WorkPoolSpec, fork_join_map, SyncRef};
pub use lazy::ParLazy;
pub use eager::ParEager;
