use crate::utils::MapFn;
use std::sync::Arc;

#[derive(Clone)]
pub struct RakeCompress<N, H, R> {
    // Function to create the initial heap value for a node
    pub(crate) impl_rake_null: Arc<dyn Fn(&N) -> H + Send + Sync>,
    pub(crate) impl_rake_add: Arc<dyn Fn(&mut H, &R) + Send + Sync>,
    pub(crate) impl_compress: Arc<dyn Fn(&H) -> R + Send + Sync>,
}
impl<N, H, R> RakeCompress<N, H, R> 
where
    N: 'static,
{
    pub fn new<F1, F2, F3>(
        rake_null: F1,
        rake_add: F2,
        compress: F3,
    ) -> Self
    where
        F1: Fn(&N) -> H + Send + Sync + 'static,
        F2: Fn(&mut H, &R) + Send + Sync + 'static,
        F3: Fn(&H) -> R + Send + Sync + 'static,
    {
        RakeCompress {
            impl_rake_null: Arc::from(Box::new(rake_null) as Box<dyn Fn(&N) -> H + Send + Sync>),
            impl_rake_add: Arc::from(Box::new(rake_add) as Box<dyn Fn(&mut H, &R) + Send + Sync>),
            impl_compress: Arc::from(Box::new(compress) as Box<dyn Fn(&H) -> R + Send + Sync>),
        }
    }
    
    pub fn rake_null(&self, node: &N) -> H {
        (self.impl_rake_null)(node)
    }
    
    pub fn rake_add(&self, heap: &mut H, result: &R) {
        (self.impl_rake_add)(heap, result)
    }
    
    pub fn compress(&self, heap: &H) -> R {
        (self.impl_compress)(heap)
    }
    
    pub fn map_rake_null<F>(&self, mapper: F) -> Self
    where 
        H: 'static,
        R: 'static,
        F: MapFn<Box<dyn Fn(&N) -> H + Send + Sync>>,
    {
        super::transformations::map_rake_null(self, mapper)
    }
    
    pub fn map_rake_add<F>(&self, mapper: F) -> Self
    where 
        H: 'static,
        R: 'static,
        F: MapFn<Box<dyn Fn(&mut H, &R) + Send + Sync>>,
    {
        super::transformations::map_rake_add(self, mapper)
    }
    
    pub fn map_compress<F>(&self, mapper: F) -> Self
    where 
        H: 'static,
        R: 'static,
        F: MapFn<Box<dyn Fn(&H) -> R + Send + Sync>>,
    {
        super::transformations::map_compress(self, mapper)
    }
    
    pub fn map<RNew, MapF, BackF>(&self, mapper: MapF, backmapper: BackF) -> RakeCompress<N, H, RNew>
    where
        H: 'static,
        R: 'static,
        RNew: 'static,
        MapF: Fn(&R) -> RNew + Send + Sync + 'static,
        BackF: Fn(&RNew) -> R + Send + Sync + 'static,
    {
        let impl_rake_null = self.impl_rake_null.clone();
        let impl_rake_add = self.impl_rake_add.clone();
        let impl_compress = self.impl_compress.clone();

        let cloned_rake_null = impl_rake_null.clone();
        let cloned_rake_add = impl_rake_add.clone();
        let cloned_impl_compress = impl_compress.clone();

        RakeCompress::new(
            move |node| {
                cloned_rake_null(node)
            },
            move |heap, result| {
                let result_old = backmapper(result);
                cloned_rake_add(heap, &result_old);
            },
            move |heap| {
                let result_old = cloned_impl_compress(heap);
                mapper(&result_old)
            },
        )
    }
    
    pub fn zipmap<RZip, MapF>(&self, mapper: MapF) -> RakeCompress<N, H, (R, RZip)>
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




