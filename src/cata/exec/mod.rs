//! Executor: the strategy for recursive tree computation.
//!
//! `Exec<D, S>` is the sole user-facing interface. D is the domain
//! (determines fold/graph types). S is the executor strategy.
//!
//! Every executor — Spec or Session — implements both `ExecutorSpec`
//! (lifecycle) and `Executor` (computation). Specs route their
//! `Executor::run` through `with_session`; Sessions are direct.
//!
//! Usage — one shape for all executors:
//! ```ignore
//! use hylic::domain::shared as dom;
//!
//! // Zero-resource (Fused): direct recursion
//! dom::FUSED.run(&fold, &graph, &root);
//!
//! // Resource (Funnel): Spec creates scoped pool internally
//! dom::exec(funnel::Spec::default(8)).run(&fold, &graph, &root);
//!
//! // Session reuse: .with() enters session scope, .run() inside is cheap
//! dom::exec(funnel::Spec::default(8)).with(|s| {
//!     s.run(&fold, &graph, &root);
//! });
//!
//! // Expert: explicit pool, bind → session
//! funnel::Pool::with(8, |pool| {
//!     dom::exec(funnel::Spec::default(8).bind(pool)).run(&fold, &graph, &root);
//! });
//! ```

pub mod variant;

pub use variant::fused;
pub use variant::funnel;

use std::marker::PhantomData;
use crate::domain::Domain;
use crate::ops::LiftOps;

// ── Core trait: computation ─────────────────────────

// ANCHOR: executor_trait
/// What a session (or spec) can do: run a fold on a tree.
/// Every type that appears as S in Exec<D, S> implements this.
pub trait Executor<N: 'static, R: 'static, D: Domain<N>> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R;

    fn run_lifted<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> R0
    where
        D: Domain<N0>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        lift.unwrap(self.run(&lifted_fold, &lifted_treeish, &lifted_root))
    }

    fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, H: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> (R0, R)
    where
        D: Domain<N0>,
        R: Clone,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        let lifted_fold = lift.lift_fold(fold.clone());
        let lifted_treeish = lift.lift_treeish(graph.clone());
        let lifted_root = lift.lift_root(root);
        let inner = self.run(&lifted_fold, &lifted_treeish, &lifted_root);
        (lift.unwrap(inner.clone()), inner)
    }
}
// ANCHOR_END: executor_trait

// ── Core trait: lifecycle ───────────────────────────

// ANCHOR: executor_spec
/// How to create/access the execution context.
/// Every type that appears as S in Exec<D, S> implements this.
///
/// - Specs: `with_session` creates the context (e.g. thread pool).
/// - Sessions: `with_session` is identity (context already exists).
/// - Zero-resource (Fused, Rayon): Session = Self, identity.
pub trait ExecutorSpec {
    type Session<'s>: 's where Self: 's;
    fn with_session<R>(&self, f: impl for<'s> FnOnce(&Self::Session<'s>) -> R) -> R;
}
// ANCHOR_END: executor_spec

// ── Exec<D, S>: the sole user-facing wrapper ───────

// ANCHOR: exec_struct
#[repr(transparent)]
pub struct Exec<D, S>(pub(crate) S, PhantomData<D>);

impl<D, S> Exec<D, S> {
    pub const fn new(inner: S) -> Self { Exec(inner, PhantomData) }
    pub fn inner(&self) -> &S { &self.0 }
}
// ANCHOR_END: exec_struct

/// Safe reinterpret: &T → &Exec<D, T> via repr(transparent).
fn wrap_ref<D, T>(inner: &T) -> &Exec<D, T> {
    // SAFETY: Exec is repr(transparent) over T. PhantomData<D> is ZST.
    unsafe { &*(inner as *const T as *const Exec<D, T>) }
}

// ── Inherent: run (the ONE way to execute) ──────────

// ANCHOR: inherent_run
impl<D, S> Exec<D, S> {
    pub fn run<N: 'static, H: 'static, R: 'static>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &<D as Domain<N>>::Treeish, root: &N,
    ) -> R
    where D: Domain<N>, S: Executor<N, R, D>
    {
        Executor::<N, R, D>::run(&self.0, fold, graph, root)
    }
// ANCHOR_END: inherent_run

    pub fn run_lifted<N0: 'static, H0: 'static, R0: 'static, N: 'static, H: 'static, R: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> R0
    where
        D: Domain<N> + Domain<N0>,
        S: Executor<N, R, D>,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        Executor::<N, R, D>::run_lifted(&self.0, lift, fold, graph, root)
    }

    pub fn run_lifted_zipped<N0: 'static, H0: 'static, R0: 'static, N: 'static, H: 'static, R: 'static>(
        &self,
        lift: &impl LiftOps<D, N0, H0, R0, N, H, R>,
        fold: &<D as Domain<N0>>::Fold<H0, R0>,
        graph: &<D as Domain<N0>>::Treeish,
        root: &N0,
    ) -> (R0, R)
    where
        D: Domain<N> + Domain<N0>,
        S: Executor<N, R, D>,
        R: Clone,
        <D as Domain<N0>>::Fold<H0, R0>: Clone,
        <D as Domain<N0>>::Treeish: Clone,
    {
        Executor::<N, R, D>::run_lifted_zipped(&self.0, lift, fold, graph, root)
    }
}

// ── Blanket: Exec implements Executor for generic code ──

impl<N: 'static, R: 'static, D: Domain<N>, S: Executor<N, R, D>> Executor<N, R, D> for Exec<D, S> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        Executor::<N, R, D>::run(&self.0, fold, graph, root)
    }
}

// ── Session scoping (for amortization) ──────────────

// ANCHOR: exec_with
impl<D, S: ExecutorSpec> Exec<D, S> {
    pub fn with<R>(&self, f: impl for<'s> FnOnce(&Exec<D, S::Session<'s>>) -> R) -> R {
        self.0.with_session(|session| f(wrap_ref(session)))
    }
}
// ANCHOR_END: exec_with
