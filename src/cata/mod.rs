pub mod exec;
pub mod lift;

#[cfg(test)]
mod tests;

pub use exec::{Executor, Exec, Fused, Sequential, Rayon, Custom};
pub use lift::Lift;
