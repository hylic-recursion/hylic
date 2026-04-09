//! FoldOps — the fold operations abstraction.
//!
//! Any type with init/accumulate/finalize. The universal interface
//! for executors.

// ANCHOR: foldops_trait
/// The three fold operations, independent of storage.
pub trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}
// ANCHOR_END: foldops_trait
