//! Owned domain — Box-based storage.
//!
//! Not Clone, not Send+Sync. The lightest domain — zero refcount.
//! Implements `FoldTransformsByValue` (consume-self); every sugar
//! is a one-line inherent wrapper over `map_phases`.

#![allow(missing_docs)] // implementation surface; items documented at the trait/type they implement

pub mod edgy;

use crate::ops::{FoldOps, FoldTransformsByValue};
use crate::exec::{Exec, fused};

// ── Executor constants (domain-bound) ────────────

pub const FUSED: Exec<super::Owned, fused::Spec> = Exec::new(fused::Spec);

/// Bind any executor to the Owned domain.
pub const fn exec<S>(s: S) -> Exec<super::Owned, S> { Exec::new(s) }

// ── Fold ──────────────────────────────────────────

pub struct Fold<N, H, R> {
    impl_init: Box<dyn Fn(&N) -> H>,
    impl_accumulate: Box<dyn Fn(&mut H, &R)>,
    impl_finalize: Box<dyn Fn(&H) -> R>,
}

impl<N: 'static, H: 'static, R: 'static> Fold<N, H, R> {
    pub fn new(
        init: impl Fn(&N) -> H + 'static,
        accumulate: impl Fn(&mut H, &R) + 'static,
        finalize: impl Fn(&H) -> R + 'static,
    ) -> Self {
        Fold {
            impl_init: Box::new(init),
            impl_accumulate: Box::new(accumulate),
            impl_finalize: Box::new(finalize),
        }
    }

    pub fn init(&self, node: &N) -> H { (self.impl_init)(node) }
    pub fn accumulate(&self, heap: &mut H, result: &R) { (self.impl_accumulate)(heap, result) }
    pub fn finalize(&self, heap: &H) -> R { (self.impl_finalize)(heap) }
}

impl<N: 'static, H: 'static, R: 'static> FoldOps<N, H, R> for Fold<N, H, R> {
    fn init(&self, node: &N) -> H { (self.impl_init)(node) }
    fn accumulate(&self, heap: &mut H, result: &R) { (self.impl_accumulate)(heap, result) }
    fn finalize(&self, heap: &H) -> R { (self.impl_finalize)(heap) }
}

// ── FoldTransformsByValue impl — Box version, consumes self ──

impl<N, H, R> FoldTransformsByValue<N, H, R> for Fold<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    type Init = Box<dyn Fn(&N) -> H>;
    type Acc  = Box<dyn Fn(&mut H, &R)>;
    type Fin  = Box<dyn Fn(&H) -> R>;

    type Out<N2, H2, R2> = Fold<N2, H2, R2> where N2: 'static, H2: 'static, R2: 'static;

    type OutInit<N2, H2> = Box<dyn Fn(&N2) -> H2> where N2: 'static, H2: 'static;
    type OutAcc<H2, R2>  = Box<dyn Fn(&mut H2, &R2)> where H2: 'static, R2: 'static;
    type OutFin<H2, R2>  = Box<dyn Fn(&H2) -> R2>   where H2: 'static, R2: 'static;

    fn map_phases<N2, H2, R2, MI, MA, MF>(
        self,
        map_init: MI,
        map_acc:  MA,
        map_fin:  MF,
    ) -> Fold<N2, H2, R2>
    where
        N2: 'static, H2: 'static, R2: 'static,
        MI: FnOnce(Self::Init) -> Self::OutInit<N2, H2>,
        MA: FnOnce(Self::Acc)  -> Self::OutAcc<H2, R2>,
        MF: FnOnce(Self::Fin)  -> Self::OutFin<H2, R2>,
    {
        Fold {
            impl_init:       map_init(self.impl_init),
            impl_accumulate: map_acc(self.impl_accumulate),
            impl_finalize:   map_fin(self.impl_finalize),
        }
    }
}

// ── Inherent sugar methods — one-liners over map_phases ──

impl<N: 'static, H: 'static, R: 'static> Fold<N, H, R> {
    pub fn wrap_init<W>(self, wrapper: W) -> Self
    where W: Fn(&N, &dyn Fn(&N) -> H) -> H + 'static,
    {
        <Self as FoldTransformsByValue<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| Box::new(crate::domain::fold_combinators::wrap_init(init, wrapper)),
            |acc| acc,
            |fin| fin,
        )
    }

    pub fn wrap_accumulate<W>(self, wrapper: W) -> Self
    where W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + 'static,
    {
        <Self as FoldTransformsByValue<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| init,
            |acc| Box::new(crate::domain::fold_combinators::wrap_accumulate(acc, wrapper)),
            |fin| fin,
        )
    }

    pub fn wrap_finalize<W>(self, wrapper: W) -> Self
    where W: Fn(&H, &dyn Fn(&H) -> R) -> R + 'static,
    {
        <Self as FoldTransformsByValue<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| init,
            |acc| acc,
            |fin| Box::new(crate::domain::fold_combinators::wrap_finalize(fin, wrapper)),
        )
    }

    pub fn map_r_bi<RNew: 'static>(self, mapper: impl Fn(&R) -> RNew + 'static, backmapper: impl Fn(&RNew) -> R + 'static) -> Fold<N, H, RNew> {
        <Self as FoldTransformsByValue<N, H, R>>::map_phases::<N, H, RNew, _, _, _>(
            self,
            |init| init,
            |acc|  Box::new(move |h: &mut H, r: &RNew| acc(h, &backmapper(r))),
            |fin|  Box::new(move |h: &H| mapper(&fin(h))),
        )
    }

    pub fn zipmap<RZip: 'static>(self, mapper: impl Fn(&R) -> RZip + 'static) -> Fold<N, H, (R, RZip)>
    where R: Clone,
    {
        self.map_r_bi(move |x| (x.clone(), mapper(x)), |x: &(R, RZip)| x.0.clone())
    }

    pub fn contramap_n<NewN: 'static>(self, f: impl Fn(&NewN) -> N + 'static) -> Fold<NewN, H, R> {
        <Self as FoldTransformsByValue<N, H, R>>::map_phases::<NewN, H, R, _, _, _>(
            self,
            |init| Box::new(move |new_n: &NewN| init(&f(new_n))),
            |acc| acc,
            |fin| fin,
        )
    }
}

pub fn fold<N: 'static, H: 'static, R: 'static>(
    init: impl Fn(&N) -> H + 'static,
    accumulate: impl Fn(&mut H, &R) + 'static,
    finalize: impl Fn(&H) -> R + 'static,
) -> Fold<N, H, R> {
    Fold::new(init, accumulate, finalize)
}

pub fn simple_fold<N: 'static, H: Clone + 'static>(
    init: impl Fn(&N) -> H + 'static,
    accumulate: impl Fn(&mut H, &H) + 'static,
) -> Fold<N, H, H> {
    Fold::new(init, accumulate, |heap| heap.clone())
}
