//! FoldOps — the fold operations abstraction.
//! FoldTransformsByRef / FoldTransformsByValue — the fold
//! transformation abstraction, split by storage mode.

// ANCHOR: foldops_trait
/// The three fold operations, independent of storage.
pub trait FoldOps<N, H, R> {
    /// Construct a fresh per-node heap from a node reference.
    fn init(&self, node: &N) -> H;
    /// Fold one child result into the heap in place.
    fn accumulate(&self, heap: &mut H, result: &R);
    /// Close out the heap into the node's final result.
    fn finalize(&self, heap: &H) -> R;
}
// ANCHOR_END: foldops_trait

/// Transformations on folds whose storage permits cheap cloning
/// (Arc / Rc). `map_phases` is the sole primitive; concrete sugars
/// (wrap_init, map, etc.) live as inherent one-line wrappers on
/// each domain's Fold type since they must construct domain-
/// specific storage (Arc::new vs Rc::new).
///
/// By-reference: takes `&self`, produces a new Fold of potentially
/// different `(N, H, R)`.
pub trait FoldTransformsByRef<N, H, R>: FoldOps<N, H, R> + Clone + Sized
where N: 'static, H: 'static, R: 'static,
{
    /// The domain's stored closure type for `init`.
    type Init;
    /// The domain's stored closure type for `accumulate`.
    type Acc;
    /// The domain's stored closure type for `finalize`.
    type Fin;

    /// The concrete Fold type in this domain over new type parameters.
    type Out<N2, H2, R2>: FoldTransformsByRef<N2, H2, R2, Init = Self::OutInit<N2, H2>, Acc = Self::OutAcc<H2, R2>, Fin = Self::OutFin<H2, R2>>
    where N2: 'static, H2: 'static, R2: 'static;

    /// Associated storage types for the output Fold's phases.
    type OutInit<N2, H2>: 'static where N2: 'static, H2: 'static;
    /// Output phase-storage type for `accumulate` after `map_phases`.
    type OutAcc<H2, R2>: 'static where H2: 'static, R2: 'static;
    /// Output phase-storage type for `finalize` after `map_phases`.
    type OutFin<H2, R2>: 'static where H2: 'static, R2: 'static;

    /// Rewrite all three phase-closures at once. Sole slot-level
    /// primitive; every sugar wraps this with appropriate phase
    /// transforms.
    fn map_phases<N2, H2, R2, MI, MA, MF>(
        &self,
        map_init: MI,
        map_acc:  MA,
        map_fin:  MF,
    ) -> Self::Out<N2, H2, R2>
    where
        N2: 'static, H2: 'static, R2: 'static,
        MI: FnOnce(Self::Init) -> Self::OutInit<N2, H2>,
        MA: FnOnce(Self::Acc)  -> Self::OutAcc<H2, R2>,
        MF: FnOnce(Self::Fin)  -> Self::OutFin<H2, R2>;
}

/// Transformations on folds whose storage is single-owner (Box).
/// `map_phases` consumes `self`; no cloning is possible.
#[allow(missing_docs)] // parallels FoldTransformsByRef; items documented there
pub trait FoldTransformsByValue<N, H, R>: FoldOps<N, H, R> + Sized
where N: 'static, H: 'static, R: 'static,
{
    type Init;
    type Acc;
    type Fin;

    type Out<N2, H2, R2>: FoldTransformsByValue<N2, H2, R2>
    where N2: 'static, H2: 'static, R2: 'static;

    type OutInit<N2, H2>: 'static where N2: 'static, H2: 'static;
    type OutAcc<H2, R2>: 'static where H2: 'static, R2: 'static;
    type OutFin<H2, R2>: 'static where H2: 'static, R2: 'static;

    fn map_phases<N2, H2, R2, MI, MA, MF>(
        self,
        map_init: MI,
        map_acc:  MA,
        map_fin:  MF,
    ) -> Self::Out<N2, H2, R2>
    where
        N2: 'static, H2: 'static, R2: 'static,
        MI: FnOnce(Self::Init) -> Self::OutInit<N2, H2>,
        MA: FnOnce(Self::Acc)  -> Self::OutAcc<H2, R2>,
        MF: FnOnce(Self::Fin)  -> Self::OutFin<H2, R2>;
}
