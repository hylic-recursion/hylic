//! ShapeLift — the universal shape-lift struct.
//!
//! One struct, one polymorphic `Lift<D, N, H, R>` impl, three
//! type-erased transformers (grow-side, treeish-side, fold-side).
//! Concrete library shape-lifts (wrap_init, map_r, explainer,
//! contramap_n, inline, …) are constructor functions on the
//! per-capable domain types (`Shared::…`, `Local::…`).
//!
//! Full design: `technical-insights/09-unified-shape-lift.md`.

use crate::domain::Domain;
use crate::ops::lift::capability::ShapeCapable;
use crate::ops::lift::core::Lift;

// ANCHOR: shape_lift_struct
/// The universal library `Lift` — stores one xform per slot
/// (grow, treeish, fold) and applies them during `apply`. Every
/// library lift except `SeedLift` is a `ShapeLift` with appropriate
/// xforms.
#[must_use = "a ShapeLift performs no work on its own; apply it via a pipeline or LiftBare"]
pub struct ShapeLift<D, N, H, R, N2, H2, R2>
where D: ShapeCapable<N> + Domain<N2>,
      N:  Clone + 'static, H:  Clone + 'static, R:  Clone + 'static,
      N2: Clone + 'static, H2: Clone + 'static, R2: Clone + 'static,
{
    pub(crate) grow_xform:    D::GrowXform<N2>,
    pub(crate) treeish_xform: D::TreeishXform<N2>,
    pub(crate) fold_xform:    D::FoldXform<H, R, N2, H2, R2>,
}
// ANCHOR_END: shape_lift_struct

impl<D, N, H, R, N2, H2, R2> Clone for ShapeLift<D, N, H, R, N2, H2, R2>
where D: ShapeCapable<N> + Domain<N2>,
      N:  Clone + 'static, H:  Clone + 'static, R:  Clone + 'static,
      N2: Clone + 'static, H2: Clone + 'static, R2: Clone + 'static,
{
    fn clone(&self) -> Self {
        ShapeLift {
            grow_xform:    self.grow_xform.clone(),
            treeish_xform: self.treeish_xform.clone(),
            fold_xform:    self.fold_xform.clone(),
        }
    }
}

impl<D, N, H, R, N2, H2, R2> ShapeLift<D, N, H, R, N2, H2, R2>
where D: ShapeCapable<N> + Domain<N2>,
      N:  Clone + 'static, H:  Clone + 'static, R:  Clone + 'static,
      N2: Clone + 'static, H2: Clone + 'static, R2: Clone + 'static,
{
    /// Construct a `ShapeLift` from per-slot xforms. Normally
    /// users call one of the domain-level convenience constructors
    /// (e.g. `Shared::wrap_init_lift`) rather than this directly.
    pub fn new(
        grow_xform:    D::GrowXform<N2>,
        treeish_xform: D::TreeishXform<N2>,
        fold_xform:    D::FoldXform<H, R, N2, H2, R2>,
    ) -> Self {
        ShapeLift { grow_xform, treeish_xform, fold_xform }
    }
}

impl<D, N, H, R, N2, H2, R2> Lift<D, N, H, R>
    for ShapeLift<D, N, H, R, N2, H2, R2>
where D: ShapeCapable<N> + Domain<N2>,
      N:  Clone + 'static, H:  Clone + 'static, R:  Clone + 'static,
      N2: Clone + 'static, H2: Clone + 'static, R2: Clone + 'static,
{
    type N2   = N2;
    type MapH = H2;
    type MapR = R2;

    fn apply<Seed, T>(
        &self,
        grow:    <D as Domain<N>>::Grow<Seed, N>,
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
        cont: impl FnOnce(
            <D as Domain<N2>>::Grow<Seed, N2>,
            <D as Domain<N2>>::Graph<N2>,
            <D as Domain<N2>>::Fold<H2, R2>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        cont(
            D::apply_grow_xform::<Seed, N2>(&self.grow_xform, grow),
            D::apply_treeish_xform::<N2>(&self.treeish_xform, treeish),
            D::apply_fold_xform::<H, R, N2, H2, R2>(&self.fold_xform, fold),
        )
    }
}
