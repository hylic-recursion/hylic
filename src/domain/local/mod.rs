//! Local domain — Rc-based storage.
//!
//! Clone (non-atomic refcount), not Send+Sync. Lighter than Shared
//! when parallelism isn't needed. Works with Fused and Sequential.
//!
//! Supports the full transformation API (map, zipmap, contramap, product).

use std::marker::PhantomData;
use std::rc::Rc;
use crate::ops::{FoldOps, FoldConstruct, TreeOps};
use crate::cata::exec::{fused, sequential};
use super::Local;

// ── Executor consts for this domain ───────────────

pub const FUSED:      fused::Exec<Local>      = fused::Exec(PhantomData);
pub const SEQUENTIAL: sequential::Exec<Local>  = sequential::Exec(PhantomData);

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

impl<N: 'static, H: 'static, R: 'static> FoldConstruct<N, H, R> for Fold<N, H, R> {
    type Mapped<N2: 'static, H2: 'static, R2: 'static> = Fold<N2, H2, R2>;
}

// ── Transformations ───────────────────────────────
// Same logic as shared::Fold (fold/algebra.rs), Rc instead of Arc,
// no Send+Sync bounds.

impl<N, H, R> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    pub fn map<RNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> Fold<N, H, RNew>
    where
        RNew: 'static,
        MapF: Fn(&R) -> RNew + 'static,
        BackF: Fn(&RNew) -> R + 'static,
    {
        let init = self.impl_init.clone();
        let acc = self.impl_accumulate.clone();
        let fin = self.impl_finalize.clone();
        Fold::new(
            move |node| init(node),
            move |heap, result| { acc(heap, &backmapper(result)); },
            move |heap| mapper(&fin(heap)),
        )
    }

    pub fn zipmap<RZip, MapF>(&self, mapper: MapF) -> Fold<N, H, (R, RZip)>
    where
        R: Clone,
        RZip: 'static,
        MapF: Fn(&R) -> RZip + 'static,
    {
        self.map(
            move |x| (x.clone(), mapper(x)),
            |x: &(R, RZip)| x.0.clone(),
        )
    }

    pub fn contramap<NewN: 'static>(
        &self,
        f: impl Fn(&NewN) -> N + 'static,
    ) -> Fold<NewN, H, R> {
        let init = self.impl_init.clone();
        let acc = self.impl_accumulate.clone();
        let fin = self.impl_finalize.clone();
        Fold::new(
            move |new_n: &NewN| init(&f(new_n)),
            move |h: &mut H, r: &R| acc(h, r),
            move |h: &H| fin(h),
        )
    }

    pub fn product<H2: 'static, R2: 'static>(
        &self,
        other: &Fold<N, H2, R2>,
    ) -> Fold<N, (H, H2), (R, R2)> {
        let init1 = self.impl_init.clone();
        let init2 = other.impl_init.clone();
        let acc1 = self.impl_accumulate.clone();
        let acc2 = other.impl_accumulate.clone();
        let fin1 = self.impl_finalize.clone();
        let fin2 = other.impl_finalize.clone();
        Fold::new(
            move |n: &N| (init1(n), init2(n)),
            move |heap: &mut (H, H2), child: &(R, R2)| {
                acc1(&mut heap.0, &child.0);
                acc2(&mut heap.1, &child.1);
            },
            move |heap: &(H, H2)| (fin1(&heap.0), fin2(&heap.1)),
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
