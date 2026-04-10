use std::sync::Arc;

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

    // ── Phase-wrapping ─────────────────────────────

    pub fn wrap_init(&self, wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + Send + Sync + 'static) -> Self {
        let inner = self.impl_init.clone();
        Fold {
            impl_init: Arc::new(super::combinators::wrap_init(move |n: &N| inner(n), wrapper)),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: self.impl_finalize.clone(),
        }
    }

    pub fn wrap_accumulate(&self, wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + Send + Sync + 'static) -> Self {
        let inner = self.impl_accumulate.clone();
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: Arc::new(super::combinators::wrap_accumulate(move |h: &mut H, r: &R| inner(h, r), wrapper)),
            impl_finalize: self.impl_finalize.clone(),
        }
    }

    pub fn wrap_finalize(&self, wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + Send + Sync + 'static) -> Self {
        let inner = self.impl_finalize.clone();
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: Arc::new(super::combinators::wrap_finalize(move |h: &H| inner(h), wrapper)),
        }
    }

    // ── Type-changing combinators ───────────────────

    pub fn map<RNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> Fold<N, H, RNew>
    where
        RNew: 'static,
        MapF: Fn(&R) -> RNew + Send + Sync + 'static,
        BackF: Fn(&RNew) -> R + Send + Sync + 'static,
    {
        let (i, a, f) = super::combinators::map_fold(
            { let v = self.impl_init.clone(); move |n: &N| v(n) },
            { let v = self.impl_accumulate.clone(); move |h: &mut H, r: &R| v(h, r) },
            { let v = self.impl_finalize.clone(); move |h: &H| v(h) },
            mapper, backmapper,
        );
        Fold::new(i, a, f)
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
        let (i, a, fin) = super::combinators::contramap_fold(
            { let v = self.impl_init.clone(); move |n: &N| v(n) },
            { let v = self.impl_accumulate.clone(); move |h: &mut H, r: &R| v(h, r) },
            { let v = self.impl_finalize.clone(); move |h: &H| v(h) },
            f,
        );
        Fold::new(i, a, fin)
    }

    pub fn product<H2: 'static, R2: 'static>(&self, other: &Fold<N, H2, R2>) -> Fold<N, (H, H2), (R, R2)> {
        let (i, a, f) = super::combinators::product_fold(
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
