//! Local domain — Rc-based storage.
//!
//! Clone (non-atomic refcount), not Send+Sync. Lighter than Shared
//! when parallelism isn't needed. Works with Fused and Sequential.

use std::rc::Rc;
use crate::ops::{FoldOps, TreeOps};

// ── Fold ──────────────────────────────────────────

pub struct Fold<N, H, R> {
    impl_init: Rc<dyn Fn(&N) -> H>,
    impl_accumulate: Rc<dyn Fn(&mut H, &R)>,
    impl_finalize: Rc<dyn Fn(&H) -> R>,
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

