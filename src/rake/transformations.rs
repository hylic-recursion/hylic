use std::sync::Arc;
use crate::rake::RakeCompress;
use crate::utils::MapFn;

type FuncNodeToHeap<N, H> = Box<dyn Fn(&N) -> H + Send + Sync>;
type FuncHeapResult<H, R> = Box<dyn Fn(&mut H, &R) + Send + Sync>;
type FuncHeapToResult<H, R> = Box<dyn Fn(&H) -> R + Send + Sync>;

pub fn map_rake_null<N, H, R, F>(
    rake_compress: &RakeCompress<N, H, R>, mapper: F,
) -> RakeCompress<N, H, R>
where N: 'static, H: 'static, R: 'static, F: MapFn<FuncNodeToHeap<N, H>>,
{
    let orig = rake_compress.impl_rake_null.clone();
    RakeCompress {
        impl_rake_null: Arc::from(mapper(Box::new(move |n: &N| orig(n)))),
        impl_rake_add: rake_compress.impl_rake_add.clone(),
        impl_compress: rake_compress.impl_compress.clone(),
    }
}

pub fn map_rake_add<N, H, R, F>(
    rake_compress: &RakeCompress<N, H, R>, mapper: F,
) -> RakeCompress<N, H, R>
where N: 'static, H: 'static, R: 'static, F: MapFn<FuncHeapResult<H, R>>,
{
    let orig = rake_compress.impl_rake_add.clone();
    RakeCompress {
        impl_rake_null: rake_compress.impl_rake_null.clone(),
        impl_rake_add: Arc::from(mapper(Box::new(move |h: &mut H, r: &R| orig(h, r)))),
        impl_compress: rake_compress.impl_compress.clone(),
    }
}

pub fn map_compress<N, H, R, F>(
    rake_compress: &RakeCompress<N, H, R>, mapper: F,
) -> RakeCompress<N, H, R>
where N: 'static, H: 'static, R: 'static, F: MapFn<FuncHeapToResult<H, R>>,
{
    let orig = rake_compress.impl_compress.clone();
    RakeCompress {
        impl_rake_null: rake_compress.impl_rake_null.clone(),
        impl_rake_add: rake_compress.impl_rake_add.clone(),
        impl_compress: Arc::from(mapper(Box::new(move |h: &H| orig(h)))),
    }
}
