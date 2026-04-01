//! FoldOps — the fold operations abstraction.
//!
//! [`FoldOps`]: any type with init/accumulate/finalize. The universal
//! interface for executors.
//!
//! [`FoldConstruct`]: marker for folds that support transformations
//! (map, zipmap, contramap, product). The `Mapped` GAT names the
//! result type in the same domain. Implemented by Shared and Local
//! folds (both Clone). Not by Owned (no Clone).

/// The three fold operations, independent of storage.
pub trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}

/// Marker: this fold supports domain-preserving transformations.
///
/// The `Mapped` GAT names "a fold of the same domain with different
/// type parameters." Transformation methods (map, zipmap, contramap,
/// product) are inherent on each domain's Fold type — they use
/// domain-specific closure bounds (Send+Sync for Shared, none for Local).
///
/// Generic code that needs transformable folds can bound on this trait
/// to access the `Mapped` type without naming the domain.
pub trait FoldConstruct<N: 'static, H: 'static, R: 'static>:
    FoldOps<N, H, R> + Clone + 'static
{
    /// A fold of the same domain with different type parameters.
    type Mapped<N2: 'static, H2: 'static, R2: 'static>: FoldConstruct<N2, H2, R2>;
}
