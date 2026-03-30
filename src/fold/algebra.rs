use crate::utils::MapFn;
use std::sync::Arc;

#[derive(Clone)]
pub struct Fold<N, H, R> {
    // Function to create the initial heap value for a node
    pub(crate) impl_init: Arc<dyn Fn(&N) -> H + Send + Sync>,
    pub(crate) impl_accumulate: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    pub(crate) impl_finalize: Arc<dyn Fn(&H) -> R + Send + Sync>,
}
impl<N, H, R> Fold<N, H, R> 
where
    N: 'static,
{
    pub fn new<F1, F2, F3>(
        init: F1,
        accumulate: F2,
        finalize: F3,
    ) -> Self
    where
        F1: Fn(&N) -> H + Send + Sync + 'static,
        F2: Fn(&mut H, &R) + Send + Sync + 'static,
        F3: Fn(&H) -> R + Send + Sync + 'static,
    {
        Fold {
            impl_init: Arc::from(Box::new(init) as Box<dyn Fn(&N) -> H + Send + Sync>),
            impl_accumulate: Arc::from(Box::new(accumulate) as Box<dyn Fn(&mut H, &R) + Send + Sync>),
            impl_finalize: Arc::from(Box::new(finalize) as Box<dyn Fn(&H) -> R + Send + Sync>),
        }
    }
    
    pub fn init(&self, node: &N) -> H {
        (self.impl_init)(node)
    }
    
    pub fn accumulate(&self, heap: &mut H, result: &R) {
        (self.impl_accumulate)(heap, result)
    }
    
    pub fn finalize(&self, heap: &H) -> R {
        (self.impl_finalize)(heap)
    }
    
    pub fn map_init<F>(&self, mapper: F) -> Self
    where 
        H: 'static,
        R: 'static,
        F: MapFn<Box<dyn Fn(&N) -> H + Send + Sync>>,
    {
        super::transformations::map_init(self, mapper)
    }
    
    pub fn map_accumulate<F>(&self, mapper: F) -> Self
    where 
        H: 'static,
        R: 'static,
        F: MapFn<Box<dyn Fn(&mut H, &R) + Send + Sync>>,
    {
        super::transformations::map_accumulate(self, mapper)
    }
    
    pub fn map_finalize<F>(&self, mapper: F) -> Self
    where 
        H: 'static,
        R: 'static,
        F: MapFn<Box<dyn Fn(&H) -> R + Send + Sync>>,
    {
        super::transformations::map_finalize(self, mapper)
    }
    
    pub fn map<RNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> Fold<N, H, RNew>
    where
        H: 'static,
        R: 'static,
        RNew: 'static,
        MapF: Fn(&R) -> RNew + Send + Sync + 'static,
        BackF: Fn(&RNew) -> R + Send + Sync + 'static,
    {
        let impl_init = self.impl_init.clone();
        let impl_accumulate = self.impl_accumulate.clone();
        let impl_finalize = self.impl_finalize.clone();

        let cloned_init = impl_init.clone();
        let cloned_accumulate = impl_accumulate.clone();
        let cloned_finalize = impl_finalize.clone();

        Fold::new(
            move |node| {
                cloned_init(node)
            },
            move |heap, result| {
                let result_old = backmapper(result);
                cloned_accumulate(heap, &result_old);
            },
            move |heap| {
                let result_old = cloned_finalize(heap);
                mapper(&result_old)
            },
        )
    }
    
    pub fn zipmap<RZip, MapF>(&self, mapper: MapF) -> Fold<N, H, (R, RZip)>
    where
        H: 'static,
        R: Clone + 'static,
        RZip: 'static,
        MapF: Fn(&R) -> RZip + Send + Sync + 'static,
    {
        let backmap = |x: &(R, RZip)| x.0.clone();
        
        self.map(
            move |x| (x.clone(), mapper(x)),
            backmap,
        )
    }
}




