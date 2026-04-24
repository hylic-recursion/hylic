//! N-change Local sugars — one-line wrappers over `Local::n_lift`.

#![allow(missing_docs)] // module-level: public items are per-domain/per-policy mirrors of documented primitives

use crate::domain::Local;
use crate::domain::local::edgy::{edgy_visit, Edgy};
use crate::ops::lift::shape::ShapeLift;

impl Local {
    pub fn map_n_bi_lift<N, H, R, N2, Co, Contra>(co: Co, contra: Contra)
        -> ShapeLift<Local, N, H, R, N2, H, R>
    where
        N:  Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        N2: Clone + 'static,
        Co:     Fn(&N)  -> N2 + Clone + 'static,
        Contra: Fn(&N2) -> N  + Clone + 'static,
    {
        let co_for_tree = co;
        let ca_for_tree = contra.clone();
        let ca_for_fold = contra;
        Local::n_lift::<N, H, R, N2, _, _>(
            move |base: &Edgy<N, N>| -> Edgy<N2, N2> {
                let base = base.clone();
                let co = co_for_tree.clone();
                let ca = ca_for_tree.clone();
                edgy_visit(move |n2: &N2, cb: &mut dyn FnMut(&N2)| {
                    let n: N = ca(n2);
                    base.visit(&n, &mut |child: &N| cb(&co(child)));
                })
            },
            ca_for_fold,
        )
    }
}
