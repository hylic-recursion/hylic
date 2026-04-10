//! Owned domain — Box-based storage.
//!
//! Not Clone, not Send+Sync. The lightest domain — zero refcount.
//! Transformations consume self (move semantics).

use crate::ops::{FoldOps, TreeOps};
use crate::cata::exec::{Exec, fused};

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

    // ── Phase-wrapping (consume self) ──────────────

    pub fn wrap_init(self, wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + 'static) -> Self {
        Fold {
            impl_init: Box::new(crate::fold::combinators::wrap_init(self.impl_init, wrapper)),
            impl_accumulate: self.impl_accumulate,
            impl_finalize: self.impl_finalize,
        }
    }

    pub fn wrap_accumulate(self, wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + 'static) -> Self {
        Fold {
            impl_init: self.impl_init,
            impl_accumulate: Box::new(crate::fold::combinators::wrap_accumulate(self.impl_accumulate, wrapper)),
            impl_finalize: self.impl_finalize,
        }
    }

    pub fn wrap_finalize(self, wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + 'static) -> Self {
        Fold {
            impl_init: self.impl_init,
            impl_accumulate: self.impl_accumulate,
            impl_finalize: Box::new(crate::fold::combinators::wrap_finalize(self.impl_finalize, wrapper)),
        }
    }

    // ── Type-changing combinators (consume self) ───

    pub fn map<RNew: 'static>(self, mapper: impl Fn(&R) -> RNew + 'static, backmapper: impl Fn(&RNew) -> R + 'static) -> Fold<N, H, RNew> {
        let (i, a, f) = crate::fold::combinators::map_fold(
            self.impl_init, self.impl_accumulate, self.impl_finalize,
            mapper, backmapper,
        );
        Fold::new(i, a, f)
    }

    pub fn zipmap<RZip: 'static>(self, mapper: impl Fn(&R) -> RZip + 'static) -> Fold<N, H, (R, RZip)>
    where R: Clone,
    {
        self.map(move |x| (x.clone(), mapper(x)), |x: &(R, RZip)| x.0.clone())
    }

    pub fn contramap<NewN: 'static>(self, f: impl Fn(&NewN) -> N + 'static) -> Fold<NewN, H, R> {
        let (i, a, fin) = crate::fold::combinators::contramap_fold(
            self.impl_init, self.impl_accumulate, self.impl_finalize, f,
        );
        Fold::new(i, a, fin)
    }

    pub fn product<H2: 'static, R2: 'static>(self, other: Fold<N, H2, R2>) -> Fold<N, (H, H2), (R, R2)> {
        let (i, a, f) = crate::fold::combinators::product_fold(
            self.impl_init, self.impl_accumulate, self.impl_finalize,
            other.impl_init, other.impl_accumulate, other.impl_finalize,
        );
        Fold::new(i, a, f)
    }
}

impl<N: 'static, H: 'static, R: 'static> FoldOps<N, H, R> for Fold<N, H, R> {
    fn init(&self, node: &N) -> H { (self.impl_init)(node) }
    fn accumulate(&self, heap: &mut H, result: &R) { (self.impl_accumulate)(heap, result) }
    fn finalize(&self, heap: &H) -> R { (self.impl_finalize)(heap) }
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

// ── Treeish ───────────────────────────────────────

pub struct Treeish<N> {
    impl_visit: Box<dyn Fn(&N, &mut dyn FnMut(&N))>,
}

impl<N: 'static> Treeish<N> {
    pub fn new(func: impl Fn(&N, &mut dyn FnMut(&N)) + 'static) -> Self {
        Treeish { impl_visit: Box::new(func) }
    }

    pub fn filter(self, pred: impl Fn(&N) -> bool + 'static) -> Self {
        Treeish::new(crate::graph::combinators::filter_edges(self.impl_visit, pred))
    }

    pub fn treemap<NewN: 'static>(
        self,
        co_tf: impl Fn(&N) -> NewN + 'static,
        contra_tf: impl Fn(&NewN) -> N + 'static,
    ) -> Treeish<NewN> {
        Treeish::new(crate::graph::combinators::treemap(self.impl_visit, co_tf, contra_tf))
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
