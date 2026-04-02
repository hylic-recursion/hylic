use crate::domain::Domain;
use crate::ops::LiftOps;

/// A paired transformation that lifts Treeish + Fold to a different
/// type domain. Purely a transformation — knows nothing about execution.
/// Use `Executor::run_lifted` to execute a lifted computation.
///
/// Domain-generic: parameterized by D. Input and output share the same
/// domain. Box storage — no Clone, no Send, no Sync required.
// ANCHOR: lift_struct
pub struct Lift<D, N, H, R, N2, H2, R2>
where
    N: 'static, H: 'static, R: 'static,
    N2: 'static, H2: 'static, R2: 'static,
    D: Domain<N> + Domain<N2>,
{
    pub(crate) impl_lift_treeish: Box<dyn Fn(<D as Domain<N>>::Treeish) -> <D as Domain<N2>>::Treeish>,
    pub(crate) impl_lift_fold: Box<dyn Fn(<D as Domain<N>>::Fold<H, R>) -> <D as Domain<N2>>::Fold<H2, R2>>,
    pub(crate) impl_lift_root: Box<dyn Fn(&N) -> N2>,
    pub(crate) impl_unwrap: Box<dyn Fn(R2) -> R>,
}
// ANCHOR_END: lift_struct

impl<D, N, H, R, N2, H2, R2> Lift<D, N, H, R, N2, H2, R2>
where
    N: 'static, H: 'static, R: 'static,
    N2: 'static, H2: 'static, R2: 'static,
    D: Domain<N> + Domain<N2>,
{
    pub fn new(
        lift_treeish: impl Fn(<D as Domain<N>>::Treeish) -> <D as Domain<N2>>::Treeish + 'static,
        lift_fold: impl Fn(<D as Domain<N>>::Fold<H, R>) -> <D as Domain<N2>>::Fold<H2, R2> + 'static,
        lift_root: impl Fn(&N) -> N2 + 'static,
        unwrap: impl Fn(R2) -> R + 'static,
    ) -> Self {
        Lift {
            impl_lift_treeish: Box::new(lift_treeish),
            impl_lift_fold: Box::new(lift_fold),
            impl_lift_root: Box::new(lift_root),
            impl_unwrap: Box::new(unwrap),
        }
    }

    /// Further transform the lifted fold. Consumes self.
    pub fn map_lifted_fold(self, mapper: impl Fn(<D as Domain<N2>>::Fold<H2, R2>) -> <D as Domain<N2>>::Fold<H2, R2> + 'static) -> Self {
        let orig = self.impl_lift_fold;
        Lift {
            impl_lift_treeish: self.impl_lift_treeish,
            impl_lift_fold: Box::new(move |fold| mapper(orig(fold))),
            impl_lift_root: self.impl_lift_root,
            impl_unwrap: self.impl_unwrap,
        }
    }

    /// Further transform the lifted treeish. Consumes self.
    pub fn map_lifted_treeish(self, mapper: impl Fn(<D as Domain<N2>>::Treeish) -> <D as Domain<N2>>::Treeish + 'static) -> Self {
        let orig = self.impl_lift_treeish;
        Lift {
            impl_lift_treeish: Box::new(move |treeish| mapper(orig(treeish))),
            impl_lift_fold: self.impl_lift_fold,
            impl_lift_root: self.impl_lift_root,
            impl_unwrap: self.impl_unwrap,
        }
    }
}

impl<D, N, H, R, N2, H2, R2> LiftOps<D, N, H, R, N2, H2, R2>
    for Lift<D, N, H, R, N2, H2, R2>
where
    N: 'static, H: 'static, R: 'static,
    N2: 'static, H2: 'static, R2: 'static,
    D: Domain<N> + Domain<N2>,
{
    fn lift_treeish(&self, t: <D as Domain<N>>::Treeish) -> <D as Domain<N2>>::Treeish {
        (self.impl_lift_treeish)(t)
    }
    fn lift_fold(&self, f: <D as Domain<N>>::Fold<H, R>) -> <D as Domain<N2>>::Fold<H2, R2> {
        (self.impl_lift_fold)(f)
    }
    fn lift_root(&self, root: &N) -> N2 {
        (self.impl_lift_root)(root)
    }
    fn unwrap(&self, result: R2) -> R {
        (self.impl_unwrap)(result)
    }
}
