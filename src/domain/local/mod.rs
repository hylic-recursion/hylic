//! Local domain — Rc-based storage.
//!
//! Clone (non-atomic refcount), not Send+Sync. Lighter than Shared
//! when parallelism isn't needed. Works with Fused (sequential only).
//!
//! Implements `FoldTransformsByRef` — same trait as Shared; storage
//! differs (Rc vs Arc + Send+Sync). Sugars (wrap_init, map, zipmap,
//! contramap) are one-line inherent wrappers over `map_phases`.

pub mod edgy;
pub mod shape_capable;
pub mod shape_lifts;

use std::rc::Rc;
use crate::ops::{FoldOps, FoldTransformsByRef};
use crate::cata::exec::{Exec, fused};

// ── Executor constants (domain-bound) ────────────

pub const FUSED: Exec<super::Local, fused::Spec> = Exec::new(fused::Spec);

/// Bind any executor to the Local domain.
pub const fn exec<S>(s: S) -> Exec<super::Local, S> { Exec::new(s) }

// ── Fold ──────────────────────────────────────────

pub struct Fold<N, H, R> {
    pub(crate) impl_init: Rc<dyn Fn(&N) -> H>,
    pub(crate) impl_accumulate: Rc<dyn Fn(&mut H, &R)>,
    pub(crate) impl_finalize: Rc<dyn Fn(&H) -> R>,
}

impl<N, H, R> Clone for Fold<N, H, R> {
    fn clone(&self) -> Self {
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: self.impl_finalize.clone(),
        }
    }
}

impl<N: 'static, H: 'static, R: 'static> Fold<N, H, R> {
    pub fn new(
        init: impl Fn(&N) -> H + 'static,
        accumulate: impl Fn(&mut H, &R) + 'static,
        finalize: impl Fn(&H) -> R + 'static,
    ) -> Self {
        Fold {
            impl_init: Rc::new(init),
            impl_accumulate: Rc::new(accumulate),
            impl_finalize: Rc::new(finalize),
        }
    }

    pub fn init(&self, node: &N) -> H { (self.impl_init)(node) }
    pub fn accumulate(&self, heap: &mut H, result: &R) { (self.impl_accumulate)(heap, result) }
    pub fn finalize(&self, heap: &H) -> R { (self.impl_finalize)(heap) }
}

impl<N: 'static, H: 'static, R: 'static> FoldOps<N, H, R> for Fold<N, H, R> {
    fn init(&self, node: &N) -> H { Fold::init(self, node) }
    fn accumulate(&self, heap: &mut H, result: &R) { Fold::accumulate(self, heap, result) }
    fn finalize(&self, heap: &H) -> R { Fold::finalize(self, heap) }
}

// ── FoldTransformsByRef impl — Rc version ──

impl<N, H, R> FoldTransformsByRef<N, H, R> for Fold<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    type Init = Rc<dyn Fn(&N) -> H>;
    type Acc  = Rc<dyn Fn(&mut H, &R)>;
    type Fin  = Rc<dyn Fn(&H) -> R>;

    type Out<N2, H2, R2> = Fold<N2, H2, R2> where N2: 'static, H2: 'static, R2: 'static;

    type OutInit<N2, H2> = Rc<dyn Fn(&N2) -> H2> where N2: 'static, H2: 'static;
    type OutAcc<H2, R2>  = Rc<dyn Fn(&mut H2, &R2)> where H2: 'static, R2: 'static;
    type OutFin<H2, R2>  = Rc<dyn Fn(&H2) -> R2>   where H2: 'static, R2: 'static;

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

// ── Inherent sugar methods — one-liners over map_phases ──

impl<N, H, R> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    pub fn wrap_init<W>(&self, wrapper: W) -> Self
    where W: Fn(&N, &dyn Fn(&N) -> H) -> H + 'static,
    {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| Rc::new(crate::domain::fold_combinators::wrap_init(move |n: &N| init(n), wrapper)),
            |acc| acc,
            |fin| fin,
        )
    }

    pub fn wrap_accumulate<W>(&self, wrapper: W) -> Self
    where W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + 'static,
    {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| init,
            |acc| Rc::new(crate::domain::fold_combinators::wrap_accumulate(move |h: &mut H, r: &R| acc(h, r), wrapper)),
            |fin| fin,
        )
    }

    pub fn wrap_finalize<W>(&self, wrapper: W) -> Self
    where W: Fn(&H, &dyn Fn(&H) -> R) -> R + 'static,
    {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, R, _, _, _>(
            self,
            |init| init,
            |acc| acc,
            |fin| Rc::new(crate::domain::fold_combinators::wrap_finalize(move |h: &H| fin(h), wrapper)),
        )
    }

    pub fn map_r_bi<RNew: 'static>(&self, mapper: impl Fn(&R) -> RNew + 'static, backmapper: impl Fn(&RNew) -> R + 'static) -> Fold<N, H, RNew> {
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<N, H, RNew, _, _, _>(
            self,
            |init| init,
            |acc|  Rc::new(move |h: &mut H, r: &RNew| acc(h, &backmapper(r))),
            |fin|  Rc::new(move |h: &H| mapper(&fin(h))),
        )
    }

    pub fn zipmap<RZip: 'static>(&self, mapper: impl Fn(&R) -> RZip + 'static) -> Fold<N, H, (R, RZip)>
    where R: Clone,
    {
        self.map_r_bi(move |x| (x.clone(), mapper(x)), |x: &(R, RZip)| x.0.clone())
    }

    pub fn contramap_n<NewN: 'static>(&self, f: impl Fn(&NewN) -> N + 'static) -> Fold<NewN, H, R> {
        let f = Rc::new(f);
        <Self as FoldTransformsByRef<N, H, R>>::map_phases::<NewN, H, R, _, _, _>(
            self,
            {
                let f = f.clone();
                |init| Rc::new(move |new_n: &NewN| init(&f(new_n)))
            },
            |acc| acc,
            |fin| fin,
        )
    }

    // ── product — binary composition; mirrors Shared::Fold::product ──
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

// ── Constructors ──────────────────────────────────

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
