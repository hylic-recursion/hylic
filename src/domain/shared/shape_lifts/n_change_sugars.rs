//! N-change Shared sugars — one-line wrappers over `Shared::n_lift`.

use crate::domain::Shared;
use crate::graph::{edgy_visit, Edgy};
use crate::ops::lift::shape::ShapeLift;

impl Shared {
    pub fn map_n_bi_lift<N, H, R, N2, Co, Contra>(co: Co, contra: Contra)
        -> ShapeLift<Shared, N, H, R, N2, H, R>
    where
        N:  Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        N2: Clone + 'static,
        Co:     Fn(&N)  -> N2 + Send + Sync + 'static + Clone,
        Contra: Fn(&N2) -> N  + Send + Sync + 'static + Clone,
    {
        let co_for_grow   = co.clone();
        let co_for_tree   = co.clone();
        let contra_for_tr = contra.clone();
        let contra_for_fd = contra;
        Shared::n_lift::<N, H, R, N2, _, _, _>(
            co_for_grow,
            move |base: &Edgy<N, N>| -> Edgy<N2, N2> {
                let base = base.clone();
                let co = co_for_tree.clone();
                let ca = contra_for_tr.clone();
                edgy_visit(move |n2: &N2, cb: &mut dyn FnMut(&N2)| {
                    let n: N = ca(n2);
                    base.visit(&n, &mut |child: &N| cb(&co(child)));
                })
            },
            contra_for_fd,
        )
    }
}
