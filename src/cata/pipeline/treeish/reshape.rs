//! reshape — the sole TreeishPipeline primitive. Rewrites both
//! base slots consistently.

use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use super::TreeishPipeline;

impl<N, H, R> TreeishPipeline<N, H, R> {
    pub fn reshape<N2, H2, R2, FT, FF>(
        self,
        reshape_treeish: FT,
        reshape_fold:    FF,
    ) -> TreeishPipeline<N2, H2, R2>
    where
        FT: FnOnce(Treeish<N>) -> Treeish<N2>,
        FF: FnOnce(Fold<N, H, R>) -> Fold<N2, H2, R2>,
    {
        TreeishPipeline {
            treeish: reshape_treeish(self.treeish),
            fold:    reshape_fold(self.fold),
        }
    }
}
