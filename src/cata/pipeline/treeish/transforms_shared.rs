// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! TreeishPipeline sugars — Shared-domain only for now (Phase 5/5).

use std::sync::Arc;
use crate::domain::Shared;
use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use super::TreeishPipeline;

impl<N, H, R> TreeishPipeline<Shared, N, H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    pub fn map_node_bi<N2, Co, Contra>(
        self,
        co: Co,
        contra: Contra,
    ) -> TreeishPipeline<Shared, N2, H, R>
    where N2: Clone + 'static,
          Co:     Fn(&N) -> N2 + Send + Sync + 'static,
          Contra: Fn(&N2) -> N + Send + Sync + 'static,
    {
        let co = Arc::new(co);
        let contra = Arc::new(contra);
        let co_for_treeish = co.clone();
        let contra_for_treeish = contra.clone();
        let contra_for_fold = contra.clone();
        self.reshape(
            move |treeish: Treeish<N>| -> Treeish<N2> {
                treeish.contramap(move |n2: &N2| contra_for_treeish(n2))
                       .map(move |n: &N| co_for_treeish(n))
            },
            move |fold: Fold<N, H, R>| -> Fold<N2, H, R> {
                fold.contramap_n(move |n2: &N2| contra_for_fold(n2))
            },
        )
    }
}
