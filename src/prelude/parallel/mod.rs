pub mod pool;
pub mod lazy;
pub mod eager;

pub use pool::{WorkPool, WorkPoolSpec};
pub use lazy::ParLazy;
pub use eager::ParEager;
