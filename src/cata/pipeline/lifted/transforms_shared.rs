//! Shared-domain Stage-2 primitive: `before_lift` (pre-compose a
//! type-preserving lift before the existing chain).
//!
//! Post-compose (append) lives on `then_lift.rs`; user-facing sugars
//! (wrap_init, map_r_bi, …) live on the `LiftedSugarsShared` blanket
//! trait in `sugars/lifted_shared.rs`.

use crate::domain::{Domain, Shared};
use crate::ops::{ComposedLift, Lift};
use super::LiftedPipeline;
use super::super::source::TreeishSource;

impl<Base, L> LiftedPipeline<Base, L>
where
    Base: TreeishSource<Domain = Shared>,
    Shared: Domain<L::N2>,
    L: Lift<Shared,
            <Base as TreeishSource>::N,
            <Base as TreeishSource>::H,
            <Base as TreeishSource>::R>,
    L::N2:   Clone + 'static,
    L::MapH: Clone + 'static,
    L::MapR: Clone + 'static,
{
    pub fn before_lift<L0>(self, first: L0) -> LiftedPipeline<Base, ComposedLift<L0, L>>
    where L0: Lift<Shared, Base::N, Base::H, Base::R>,
          Shared: Domain<L0::N2>,
    {
        LiftedPipeline { base: self.base, pre_lift: ComposedLift::compose(first, self.pre_lift) }
    }
}
