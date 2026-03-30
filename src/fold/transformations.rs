use std::sync::Arc;
use crate::fold::Fold;
use super::{InitFn, AccumulateFn, FinalizeFn};

pub fn map_init<N, H, R, F>(
    run: &Fold<N, H, R>, mapper: F,
) -> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static, F: FnOnce(InitFn<N, H>) -> InitFn<N, H>,
{
    let orig = run.impl_init.clone();
    Fold {
        impl_init: Arc::from(mapper(Box::new(move |n: &N| orig(n)))),
        impl_accumulate: run.impl_accumulate.clone(),
        impl_finalize: run.impl_finalize.clone(),
    }
}

pub fn map_accumulate<N, H, R, F>(
    run: &Fold<N, H, R>, mapper: F,
) -> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static, F: FnOnce(AccumulateFn<H, R>) -> AccumulateFn<H, R>,
{
    let orig = run.impl_accumulate.clone();
    Fold {
        impl_init: run.impl_init.clone(),
        impl_accumulate: Arc::from(mapper(Box::new(move |h: &mut H, r: &R| orig(h, r)))),
        impl_finalize: run.impl_finalize.clone(),
    }
}

pub fn map_finalize<N, H, R, F>(
    run: &Fold<N, H, R>, mapper: F,
) -> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static, F: FnOnce(FinalizeFn<H, R>) -> FinalizeFn<H, R>,
{
    let orig = run.impl_finalize.clone();
    Fold {
        impl_init: run.impl_init.clone(),
        impl_accumulate: run.impl_accumulate.clone(),
        impl_finalize: Arc::from(mapper(Box::new(move |h: &H| orig(h)))),
    }
}
