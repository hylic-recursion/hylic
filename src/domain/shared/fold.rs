//! Shared-domain Fold — Arc-based closure storage, Clone, Send+Sync.
//!
//! One primitive: `map_phases(map_init, map_acc, map_fin)`. Every
//! phase-wrapping / type-changing sugar (`wrap_init`,
//! `wrap_accumulate`, `wrap_finalize`, `map`, `zipmap`, `contramap`)
//! is a one-line wrapper over `map_phases` plus a slot-level combinator
//! from `crate::fold::combinators`.
//!
//! `product` is the single exception: it is binary (takes another
//! `Fold`) and cannot be expressed as a single-fold `map_phases`.

use std::sync::Arc;
use crate::ops::FoldOps;
use crate::fold::combinators;

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

    // ── map_phases — sole slot-level primitive ──────

    /// Rewrite all three phase-closures at once, independently.
    /// Each sugar below is a one-line wrapper over this.
    pub fn map_phases<N2, H2, R2, MI, MA, MF>(
        &self,
        map_init: MI,
        map_acc:  MA,
        map_fin:  MF,
    ) -> Fold<N2, H2, R2>
    where
        N2: 'static, H2: 'static, R2: 'static,
        MI: FnOnce(Arc<dyn Fn(&N) -> H + Send + Sync>)
                   -> Arc<dyn Fn(&N2) -> H2 + Send + Sync>,
        MA: FnOnce(Arc<dyn Fn(&mut H, &R) + Send + Sync>)
                   -> Arc<dyn Fn(&mut H2, &R2) + Send + Sync>,
        MF: FnOnce(Arc<dyn Fn(&H) -> R + Send + Sync>)
                   -> Arc<dyn Fn(&H2) -> R2 + Send + Sync>,
    {
        Fold {
            impl_init:       map_init(self.impl_init.clone()),
            impl_accumulate: map_acc(self.impl_accumulate.clone()),
            impl_finalize:   map_fin(self.impl_finalize.clone()),
        }
    }

    // ── Phase wrappers — one-liners over map_phases ──

    pub fn wrap_init<W>(&self, wrapper: W) -> Self
    where W: Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static,
    {
        self.map_phases(
            |init| Arc::new(combinators::wrap_init(move |n: &N| init(n), wrapper)),
            |acc| acc,
            |fin| fin,
        )
    }

    pub fn wrap_accumulate<W>(&self, wrapper: W) -> Self
    where W: Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static,
    {
        self.map_phases(
            |init| init,
            |acc| Arc::new(combinators::wrap_accumulate(move |h: &mut H, r: &R| acc(h, r), wrapper)),
            |fin| fin,
        )
    }

    pub fn wrap_finalize<W>(&self, wrapper: W) -> Self
    where W: Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static,
    {
        self.map_phases(
            |init| init,
            |acc| acc,
            |fin| Arc::new(combinators::wrap_finalize(move |h: &H| fin(h), wrapper)),
        )
    }

    // ── Type-changing combinators — one-liners over map_phases ──

    pub fn map<RNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> Fold<N, H, RNew>
    where
        RNew: 'static,
        MapF:  Fn(&R) -> RNew + Send + Sync + 'static,
        BackF: Fn(&RNew) -> R + Send + Sync + 'static,
    {
        self.map_phases(
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
        self.map(move |x| (x.clone(), mapper(x)), |x: &(R, RZip)| x.0.clone())
    }

    pub fn contramap<NewN: 'static>(&self, f: impl Fn(&NewN) -> N + Send + Sync + 'static) -> Fold<NewN, H, R> {
        let f = Arc::new(f);
        self.map_phases(
            {
                let f = f.clone();
                |init| Arc::new(move |new_n: &NewN| init(&f(new_n)))
            },
            |acc| acc,
            |fin| fin,
        )
    }

    // ── product — binary; not sugar over map_phases ──

    pub fn product<H2: 'static, R2: 'static>(&self, other: &Fold<N, H2, R2>) -> Fold<N, (H, H2), (R, R2)> {
        let (i, a, f) = combinators::product_fold(
            { let v = self.impl_init.clone(); move |n: &N| v(n) },
            { let v = self.impl_accumulate.clone(); move |h: &mut H, r: &R| v(h, r) },
            { let v = self.impl_finalize.clone(); move |h: &H| v(h) },
            { let v = other.impl_init.clone(); move |n: &N| v(n) },
            { let v = other.impl_accumulate.clone(); move |h: &mut H2, r: &R2| v(h, r) },
            { let v = other.impl_finalize.clone(); move |h: &H2| v(h) },
        );
        Fold::new(i, a, f)
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
