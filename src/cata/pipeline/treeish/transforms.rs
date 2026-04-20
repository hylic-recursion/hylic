//! TreeishPipeline sugars — one-liners over reshape.
//!
//! Only `contramap_node` makes sense here: no Seed side to filter or
//! wrap_grow. N-changing via bijection cleanly reshapes both
//! treeish and fold.

use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use super::TreeishPipeline;

impl<N, H, R> TreeishPipeline<N, H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    pub fn contramap_node<N2, Co, Contra>(
        self,
        co: Co,
        contra: Contra,
    ) -> TreeishPipeline<N2, H, R>
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
                fold.contramap(move |n2: &N2| contra_for_fold(n2))
            },
        )
    }
}
