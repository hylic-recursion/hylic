//! Shared-domain Fold — Arc-based closure storage, Clone, Send+Sync.
//!
//! Implements `FoldTransformsByRef` (trait in `ops::fold`). The sole
//! primitive is `map_phases` (from the trait); every phase-wrapping
//! and type-changing sugar (`wrap_init`, `wrap_accumulate`,
//! `wrap_finalize`, `map`, `zipmap`, `contramap`) is a one-line
//! inherent wrapper over `map_phases`.
//!
//! `product` stays as a separate binary method (not a single-fold
//! transformation).

#![allow(missing_docs)] // implementation surface; items documented at the trait/type they implement

use std::sync::Arc;
use crate::ops::{FoldOps, FoldTransformsByRef};
use crate::domain::fold_combinators as combinators;

// ANCHOR: fold_struct
pub struct Fold<N, H, R> {
    pub(crate) impl_init: Arc<dyn Fn(&N) -> H + Send + Sync>,
    pub(crate) impl_accumulate: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    pub(crate) impl_finalize: Arc<dyn Fn(&H) -> R + Send + Sync>,
}
// ANCHOR_END: fold_struct

impl<N, H, R> Clone for Fold<N, H, R> {
    fn clone(&self) -> Self {
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: self.impl_finalize.clone(),
        }
    }
}

impl<N: 'static, H: 'static, R: 'static> FoldOps<N, H, R> for Fold<N, H, R> {
    fn init(&self, node: &N) -> H { Fold::init(self, node) }
    fn accumulate(&self, heap: &mut H, result: &R) { Fold::accumulate(self, heap, result) }
    fn finalize(&self, heap: &H) -> R { Fold::finalize(self, heap) }
}

// ── FoldTransformsByRef impl — the trait's map_phases body ──

impl<N, H, R> FoldTransformsByRef<N, H, R> for Fold<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    type Init = Arc<dyn Fn(&N) -> H + Send + Sync>;
    type Acc  = Arc<dyn Fn(&mut H, &R) + Send + Sync>;
    type Fin  = Arc<dyn Fn(&H) -> R + Send + Sync>;

    type Out<N2, H2, R2> = Fold<N2, H2, R2> where N2: 'static, H2: 'static, R2: 'static;

    type OutInit<N2, H2> = Arc<dyn Fn(&N2) -> H2 + Send + Sync> where N2: 'static, H2: 'static;
    type OutAcc<H2, R2>  = Arc<dyn Fn(&mut H2, &R2) + Send + Sync> where H2: 'static, R2: 'static;
    type OutFin<H2, R2>  = Arc<dyn Fn(&H2) -> R2 + Send + Sync>   where H2: 'static, R2: 'static;

    fn map_phases<N2, H2, R2, MI, MA, MF>(
        &self,
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
            impl_init:       map_init(self.impl_init.clone()),
            impl_accumulate: map_acc(self.impl_accumulate.clone()),
            impl_finalize:   map_fin(self.impl_finalize.clone()),
        }
    }
}

// ── Inherent methods — construction + direct phase access ──

impl<N, H, R> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    pub fn new<F1, F2, F3>(init: F1, accumulate: F2, finalize: F3) -> Self
    where
        F1: Fn(&N) -> H + Send + Sync + 'static,
        F2: Fn(&mut H, &R) + Send + Sync + 'static,
        F3: Fn(&H) -> R + Send + Sync + 'static,
    {
        Fold {
            impl_init: Arc::new(init),
            impl_accumulate: Arc::new(accumulate),
            impl_finalize: Arc::new(finalize),
        }
    }

    pub fn init(&self, node: &N) -> H { (self.impl_init)(node) }
    pub fn accumulate(&self, heap: &mut H, result: &R) { (self.impl_accumulate)(heap, result) }
    pub fn finalize(&self, heap: &H) -> R { (self.impl_finalize)(heap) }

    // ── Phase wrappers — one-liners over the trait's map_phases ──

    pub fn wrap_init<W>(&self, wrapper: W) -> Self
    where W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| Arc::new(combinators::wrap_init(move |n: &N| init(n), wrapper)),
            |acc| acc,
            |fin| fin,
        )
    }

    pub fn wrap_accumulate<W>(&self, wrapper: W) -> Self
    where W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| init,
            |acc| Arc::new(combinators::wrap_accumulate(move |h: &mut H, r: &R| acc(h, r), wrapper)),
            |fin| fin,
        )
    }

    pub fn wrap_finalize<W>(&self, wrapper: W) -> Self
    where W: Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| init,
            |acc| acc,
            |fin| Arc::new(combinators::wrap_finalize(move |h: &H| fin(h), wrapper)),
        )
    }

    // ── Type-changing combinators — one-liners over map_phases ──

    pub fn map_r_bi<RNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> Fold<N, H, RNew>
    where
        RNew: 'static,
        MapF:  Fn(&R) -> RNew + Send + Sync + 'static,
        BackF: Fn(&RNew) -> R + Send + Sync + 'static,
    {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, RNew, _, _, _>(
            self,
            |init| init,
            |acc|  Arc::new(move |h: &mut H, r: &RNew| acc(h, &backmapper(r))),
            |fin|  Arc::new(move |h: &H| mapper(&fin(h))),
        )
    }

    pub fn zipmap<RZip, MapF>(&self, mapper: MapF) -> Fold<N, H, (R, RZip)>
    where
        R: Clone,
        RZip: 'static,
        MapF: Fn(&R) -> RZip + Send + Sync + 'static,
    {
        self.map_r_bi(move |x| (x.clone(), mapper(x)), |x: &(R, RZip)| x.0.clone())
    }

    pub fn contramap_n<NewN: 'static>(&self, f: impl Fn(&NewN) -> N + Send + Sync + 'static) -> Fold<NewN, H, R> {
        let f = Arc::new(f);
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<NewN, H, R, _, _, _>(
            self,
            {
                let f = f.clone();
                |init| Arc::new(move |new_n: &NewN| init(&f(new_n)))
            },
            |acc| acc,
            |fin| fin,
        )
    }

    // ── product — binary composition; stays as its own method ──
    pub fn product<H2: 'static, R2: 'static>(&self, other: &Fold<N, H2, R2>)
        -> Fold<N, (H, H2), (R, R2)>
    where N: Clone,
    {
        let i1 = self.impl_init.clone();       let i2 = other.impl_init.clone();
        let a1 = self.impl_accumulate.clone(); let a2 = other.impl_accumulate.clone();
        let f1 = self.impl_finalize.clone();   let f2 = other.impl_finalize.clone();
        Fold::new(
            move |n: &N| (i1(n), i2(n)),
            move |heap: &mut (H, H2), child: &(R, R2)| {
                a1(&mut heap.0, &child.0);
                a2(&mut heap.1, &child.1);
            },
            move |heap: &(H, H2)| (f1(&heap.0), f2(&heap.1)),
        )
    }
}

// ── Constructors ───────────────────────────────────

pub fn fold<N, H, R>(
    init: impl Fn(&N) -> H + Send + Sync + 'static,
    accumulate: impl Fn(&mut H, &R) + Send + Sync + 'static,
    finalize: impl Fn(&H) -> R + Send + Sync + 'static,
) -> Fold<N, H, R> where N: 'static, H: 'static, R: 'static {
    Fold::new(init, accumulate, finalize)
}

pub fn simple_fold<N, H>(
    init: impl Fn(&N) -> H + Send + Sync + 'static,
    accumulate: impl Fn(&mut H, &H) + Send + Sync + 'static,
) -> Fold<N, H, H> where N: 'static, H: Clone + 'static {
    Fold::new(init, accumulate, |heap| heap.clone())
}
