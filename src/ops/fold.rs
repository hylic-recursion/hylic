//! FoldOps — the fold operations abstraction.
//!
//! Any type implementing init/accumulate/finalize can serve as a fold
//! for an executor. Domain-specific types (shared::Fold, local::Fold,
//! owned::Fold) implement this, as can user-defined structs for
//! zero-boxing, fully-monomorphized execution.

/// The three fold operations, independent of storage.
pub trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}
