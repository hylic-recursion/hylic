//! Local domain — Rc-based storage.
//!
//! Clone (non-atomic refcount), not Send+Sync. Lighter than Shared
//! when parallelism isn't needed. Works with Fused.
//!
//! Supports the full transformation API (map, zipmap, contramap, product).

use std::rc::Rc;
use crate::ops::{FoldOps, TreeOps};
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

// ── Transformations ───────────────────────────────
// Same logic as shared::Fold (fold/algebra.rs), Rc instead of Arc,
// no Send+Sync bounds.

impl<N, H, R> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    // ── Phase-wrapping ─────────────────────────────

    pub fn wrap_init(&self, wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + 'static) -> Self {
        let inner = self.impl_init.clone();
        Fold {
            impl_init: Rc::new(crate::fold::combinators::wrap_init(move |n: &N| inner(n), wrapper)),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: self.impl_finalize.clone(),
        }
    }

    pub fn wrap_accumulate(&self, wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + 'static) -> Self {
        let inner = self.impl_accumulate.clone();
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: Rc::new(crate::fold::combinators::wrap_accumulate(move |h: &mut H, r: &R| inner(h, r), wrapper)),
            impl_finalize: self.impl_finalize.clone(),
        }
    }

    pub fn wrap_finalize(&self, wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + 'static) -> Self {
        let inner = self.impl_finalize.clone();
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: Rc::new(crate::fold::combinators::wrap_finalize(move |h: &H| inner(h), wrapper)),
        }
    }

    // ── Type-changing combinators ───────────────────

    pub fn map<RNew: 'static>(&self, mapper: impl Fn(&R) -> RNew + 'static, backmapper: impl Fn(&RNew) -> R + 'static) -> Fold<N, H, RNew> {
        let (i, a, f) = crate::fold::combinators::map_fold(
            { let v = self.impl_init.clone(); move |n: &N| v(n) },
            { let v = self.impl_accumulate.clone(); move |h: &mut H, r: &R| v(h, r) },
            { let v = self.impl_finalize.clone(); move |h: &H| v(h) },
            mapper, backmapper,
        );
        Fold::new(i, a, f)
    }

    pub fn zipmap<RZip: 'static>(&self, mapper: impl Fn(&R) -> RZip + 'static) -> Fold<N, H, (R, RZip)>
    where R: Clone,
    {
        self.map(move |x| (x.clone(), mapper(x)), |x: &(R, RZip)| x.0.clone())
    }

    pub fn contramap<NewN: 'static>(&self, f: impl Fn(&NewN) -> N + 'static) -> Fold<NewN, H, R> {
        let (i, a, fin) = crate::fold::combinators::contramap_fold(
            { let v = self.impl_init.clone(); move |n: &N| v(n) },
            { let v = self.impl_accumulate.clone(); move |h: &mut H, r: &R| v(h, r) },
            { let v = self.impl_finalize.clone(); move |h: &H| v(h) },
            f,
        );
        Fold::new(i, a, fin)
    }

    pub fn product<H2: 'static, R2: 'static>(&self, other: &Fold<N, H2, R2>) -> Fold<N, (H, H2), (R, R2)> {
        let (i, a, f) = crate::fold::combinators::product_fold(
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

// ── Treeish ───────────────────────────────────────

pub struct Treeish<N> {
    impl_visit: Rc<dyn Fn(&N, &mut dyn FnMut(&N))>,
}

impl<N> Clone for Treeish<N> {
    fn clone(&self) -> Self { Treeish { impl_visit: self.impl_visit.clone() } }
}

impl<N: 'static> Treeish<N> {
    pub fn new(func: impl Fn(&N, &mut dyn FnMut(&N)) + 'static) -> Self {
        Treeish { impl_visit: Rc::new(func) }
    }

    pub fn filter(&self, pred: impl Fn(&N) -> bool + 'static) -> Self {
        let inner = self.impl_visit.clone();
        treeish_visit(crate::graph::combinators::filter_edges(
            move |n: &N, cb: &mut dyn FnMut(&N)| inner(n, cb), pred,
        ))
    }

    pub fn treemap<NewN: 'static>(
        &self,
        co_tf: impl Fn(&N) -> NewN + 'static,
        contra_tf: impl Fn(&NewN) -> N + 'static,
    ) -> Treeish<NewN> {
        let inner = self.impl_visit.clone();
        Treeish::new(crate::graph::combinators::treemap(
            move |n: &N, cb: &mut dyn FnMut(&N)| inner(n, cb), co_tf, contra_tf,
        ))
    }
}

impl<N: 'static> TreeOps<N> for Treeish<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N)) {
        (self.impl_visit)(node, cb)
    }
}

pub fn treeish_visit<N: 'static>(
    func: impl Fn(&N, &mut dyn FnMut(&N)) + 'static,
) -> Treeish<N> {
    Treeish::new(func)
}
