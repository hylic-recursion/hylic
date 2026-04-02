//! Owned domain — Box-based storage.
//!
//! Not Clone, not Send+Sync. The lightest domain — zero refcount.
//! Works with Fused and Sequential (they borrow, never clone).

use std::marker::PhantomData;
use crate::ops::{FoldOps, TreeOps};
use crate::cata::exec::{FusedIn, SequentialIn};
use super::Owned;

// ── Executor consts for this domain ───────────────

pub const FUSED:      FusedIn<Owned>      = FusedIn(PhantomData);
pub const SEQUENTIAL: SequentialIn<Owned>  = SequentialIn(PhantomData);

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
