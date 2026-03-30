use std::sync::Arc;
use crate::fold::Fold;
use crate::utils::MapFn;

type InitFn<N, H> = Box<dyn Fn(&N) -> H + Send + Sync>;
type AccumulateFn<H, R> = Box<dyn Fn(&mut H, &R) + Send + Sync>;
type FinalizeFn<H, R> = Box<dyn Fn(&H) -> R + Send + Sync>;

pub fn map_init<N, H, R, F>(
    run: &Fold<N, H, R>, mapper: F,
) -> Fold<N, H, R>
where N: 'static, H: 'static, R: 'static, F: MapFn<InitFn<N, H>>,
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
where N: 'static, H: 'static, R: 'static, F: MapFn<AccumulateFn<H, R>>,
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
where N: 'static, H: 'static, R: 'static, F: MapFn<FinalizeFn<H, R>>,
{
    let orig = run.impl_finalize.clone();
    Fold {
        impl_init: run.impl_init.clone(),
        impl_accumulate: run.impl_accumulate.clone(),
        impl_finalize: Arc::from(mapper(Box::new(move |h: &H| orig(h)))),
    }
}
