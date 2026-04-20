//! reshape — the sole TreeishPipeline primitive. Domain-generic.

use crate::domain::Domain;
use super::TreeishPipeline;

impl<D, N, H, R> TreeishPipeline<D, N, H, R>
where D: Domain<N>,
      N: 'static, H: 'static, R: 'static,
{
    pub fn reshape<N2, H2, R2, FT, FF>(
        self,
        reshape_treeish: FT,
        reshape_fold:    FF,
    ) -> TreeishPipeline<D, N2, H2, R2>
    where
        D: Domain<N2>,
        N2: 'static, H2: 'static, R2: 'static,
        FT: FnOnce(<D as Domain<N>>::Graph<N>)     -> <D as Domain<N2>>::Graph<N2>,
        FF: FnOnce(<D as Domain<N>>::Fold<H, R>)   -> <D as Domain<N2>>::Fold<H2, R2>,
    {
        TreeishPipeline {
            treeish: reshape_treeish(self.treeish),
            fold:    reshape_fold(self.fold),
        }
    }
}
