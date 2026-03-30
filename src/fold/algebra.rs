use std::sync::Arc;
use super::{InitFn, AccumulateFn, FinalizeFn};

pub struct Fold<N, H, R> {
    pub(crate) impl_init: Arc<dyn Fn(&N) -> H + Send + Sync>,
    pub(crate) impl_accumulate: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    pub(crate) impl_finalize: Arc<dyn Fn(&H) -> R + Send + Sync>,
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
            impl_init: Arc::from(Box::new(init) as InitFn<N, H>),
            impl_accumulate: Arc::from(Box::new(accumulate) as AccumulateFn<H, R>),
            impl_finalize: Arc::from(Box::new(finalize) as FinalizeFn<H, R>),
        }
    }

    pub fn init(&self, node: &N) -> H { (self.impl_init)(node) }
    pub fn accumulate(&self, heap: &mut H, result: &R) { (self.impl_accumulate)(heap, result) }
    pub fn finalize(&self, heap: &H) -> R { (self.impl_finalize)(heap) }

    pub fn map_init<F>(&self, mapper: F) -> Self
    where F: FnOnce(InitFn<N, H>) -> InitFn<N, H> + 'static,
    {
        let orig = self.impl_init.clone();
        Fold {
            impl_init: Arc::from(mapper(Box::new(move |n: &N| orig(n)))),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: self.impl_finalize.clone(),
        }
    }

    pub fn map_accumulate<F>(&self, mapper: F) -> Self
    where F: FnOnce(AccumulateFn<H, R>) -> AccumulateFn<H, R> + 'static,
    {
        let orig = self.impl_accumulate.clone();
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: Arc::from(mapper(Box::new(move |h: &mut H, r: &R| orig(h, r)))),
            impl_finalize: self.impl_finalize.clone(),
        }
    }

    pub fn map_finalize<F>(&self, mapper: F) -> Self
    where F: FnOnce(FinalizeFn<H, R>) -> FinalizeFn<H, R> + 'static,
    {
        let orig = self.impl_finalize.clone();
        Fold {
            impl_init: self.impl_init.clone(),
            impl_accumulate: self.impl_accumulate.clone(),
            impl_finalize: Arc::from(mapper(Box::new(move |h: &H| orig(h)))),
        }
    }

    pub fn map<RNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> Fold<N, H, RNew>
    where
        RNew: 'static,
        MapF: Fn(&R) -> RNew + Send + Sync + 'static,
        BackF: Fn(&RNew) -> R + Send + Sync + 'static,
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
        MapF: Fn(&R) -> RZip + Send + Sync + 'static,
    {
        self.map(
            move |x| (x.clone(), mapper(x)),
            |x: &(R, RZip)| x.0.clone(),
        )
    }

    /// Change the node type. Only init sees the node — accumulate and
    /// finalize are unchanged. The fold is contravariant in N.
    pub fn contramap<NewN: 'static>(
        &self,
        f: impl Fn(&NewN) -> N + Send + Sync + 'static,
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

    /// Run two folds in one tree traversal. The categorical product:
    /// each fold maintains its own heap, sees its own child results,
    /// produces its own output. One pass, two results.
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
