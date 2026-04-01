use std::sync::Arc;
use crate::graph::Treeish;
use crate::fold::Fold;

/// A paired transformation that lifts Treeish + Fold to a different
/// type domain. Purely a transformation — knows nothing about execution.
/// Use `Executor::run_lifted` to execute a lifted computation.
// ANCHOR: lift_struct
pub struct Lift<N, H, R, N2, H2, R2> {
    pub(crate) impl_lift_treeish: Arc<dyn Fn(Treeish<N>) -> Treeish<N2> + Send + Sync>,
    pub(crate) impl_lift_fold: Arc<dyn Fn(Fold<N, H, R>) -> Fold<N2, H2, R2> + Send + Sync>,
    pub(crate) impl_lift_root: Arc<dyn Fn(&N) -> N2 + Send + Sync>,
    pub(crate) impl_unwrap: Arc<dyn Fn(R2) -> R + Send + Sync>,
}
// ANCHOR_END: lift_struct

impl<N, H, R, N2, H2, R2> Clone for Lift<N, H, R, N2, H2, R2> {
    fn clone(&self) -> Self {
        Lift {
            impl_lift_treeish: self.impl_lift_treeish.clone(),
            impl_lift_fold: self.impl_lift_fold.clone(),
            impl_lift_root: self.impl_lift_root.clone(),
            impl_unwrap: self.impl_unwrap.clone(),
        }
    }
}

impl<N: 'static, H: 'static, R: 'static, N2: 'static, H2: 'static, R2: 'static>
    Lift<N, H, R, N2, H2, R2>
{
    pub fn new(
        lift_treeish: impl Fn(Treeish<N>) -> Treeish<N2> + Send + Sync + 'static,
        lift_fold: impl Fn(Fold<N, H, R>) -> Fold<N2, H2, R2> + Send + Sync + 'static,
        lift_root: impl Fn(&N) -> N2 + Send + Sync + 'static,
        unwrap: impl Fn(R2) -> R + Send + Sync + 'static,
    ) -> Self {
        Lift {
            impl_lift_treeish: Arc::new(lift_treeish),
            impl_lift_fold: Arc::new(lift_fold),
            impl_lift_root: Arc::new(lift_root),
            impl_unwrap: Arc::new(unwrap),
        }
    }

    pub fn lift_treeish(&self, treeish: Treeish<N>) -> Treeish<N2> {
        (self.impl_lift_treeish)(treeish)
    }

    pub fn lift_fold(&self, fold: Fold<N, H, R>) -> Fold<N2, H2, R2> {
        (self.impl_lift_fold)(fold)
    }

    pub fn lift_root(&self, root: &N) -> N2 {
        (self.impl_lift_root)(root)
    }

    pub fn unwrap(&self, result: R2) -> R {
        (self.impl_unwrap)(result)
    }

    /// Further transform the lifted fold without changing types.
    pub fn map_lifted_fold<F>(&self, mapper: F) -> Self
    where F: Fn(Fold<N2, H2, R2>) -> Fold<N2, H2, R2> + Send + Sync + 'static,
    {
        let orig = self.impl_lift_fold.clone();
        Lift {
            impl_lift_treeish: self.impl_lift_treeish.clone(),
            impl_lift_fold: Arc::new(move |fold| mapper(orig(fold))),
            impl_lift_root: self.impl_lift_root.clone(),
            impl_unwrap: self.impl_unwrap.clone(),
        }
    }

    /// Further transform the lifted treeish without changing types.
    pub fn map_lifted_treeish<F>(&self, mapper: F) -> Self
    where F: Fn(Treeish<N2>) -> Treeish<N2> + Send + Sync + 'static,
    {
        let orig = self.impl_lift_treeish.clone();
        Lift {
            impl_lift_treeish: Arc::new(move |treeish| mapper(orig(treeish))),
            impl_lift_fold: self.impl_lift_fold.clone(),
            impl_lift_root: self.impl_lift_root.clone(),
            impl_unwrap: self.impl_unwrap.clone(),
        }
    }
}
