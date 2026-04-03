pub mod lazy;
pub mod eager;

pub use lazy::ParLazy;
pub use eager::{ParEager, EagerSpec};
