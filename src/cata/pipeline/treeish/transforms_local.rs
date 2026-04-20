// LAYER: upper  (moves to `hylic-pipelines` crate on future split — see KB/.plans/finishing-up/next-modularization/layer-manifest.md)
//! Stage-1 Local sugar on TreeishPipeline — map_node_bi mirror.

use std::rc::Rc;
use crate::domain::Local;
use crate::domain::local::Fold;
use crate::domain::local::edgy::Edgy;
use super::TreeishPipeline;

impl<N, H, R> TreeishPipeline<Local, N, H, R>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
{
    pub fn map_node_bi_local<N2, Co, Contra>(
        self, co: Co, contra: Contra,
    ) -> TreeishPipeline<Local, N2, H, R>
    where N2: Clone + 'static,
          Co:     Fn(&N) -> N2 + 'static,
          Contra: Fn(&N2) -> N + 'static,
    {
        let co = Rc::new(co);
        let contra = Rc::new(contra);
        let co_for_treeish = co.clone();
        let contra_for_treeish = contra.clone();
        let contra_for_fold = contra;
        self.reshape(
            move |treeish: Edgy<N, N>| -> Edgy<N2, N2> {
                treeish.contramap(move |n2: &N2| contra_for_treeish(n2))
                       .map(move |n: &N| co_for_treeish(n))
            },
            move |fold: Fold<N, H, R>| -> Fold<N2, H, R> {
                fold.contramap_n(move |n2: &N2| contra_for_fold(n2))
            },
        )
    }
}
