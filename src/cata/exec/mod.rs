//! Executor: the strategy for recursive tree computation.
//!
//! `Exec<D, S>` is the sole user-facing interface. D is the domain
//! (determines fold/graph types). S is the executor strategy.
//!
//! Every Spec implements `ExecutorSpec` (lifecycle) and `Executor`
//! (computation). Sessions implement `Executor` only — they're
//! already bound to their resource.
//!
//! ```ignore
//! use hylic::domain::shared as dom;
//!
//! // One-shot: Spec creates + destroys resource internally
//! dom::exec(funnel::Spec::default(8)).run(&fold, &graph, &root);
//!
//! // Session scope: amortized multi-run
//! dom::exec(funnel::Spec::default(8)).session(|s| {
//!     s.run(&fold, &graph, &root);
//! });
//!
//! // Explicit attach: manual resource management
//! funnel::Pool::with(8, |pool| {
//!     dom::exec(funnel::Spec::default(8)).attach(pool).run(&fold, &graph, &root);
//! });
//!
//! // Provider-based attach: stable signature
//! dom::exec(funnel::Spec::default(8)).attach_from(&pools).run(&fold, &graph, &root);
//! ```

pub mod variant;

pub use variant::fused;
pub use variant::funnel;

use std::marker::PhantomData;
use crate::domain::Domain;
use crate::ops::LiftOps;

// ── Core trait: computation ─────────────────────────

// ANCHOR: executor_trait
/// Run a fold on a tree. Both Specs and Sessions implement this.
/// Spec::run routes through with_session. Session::run is direct.
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
/// Lifecycle: resource management + session creation.
/// Only Specs implement this. Sessions are the output.
///
/// - `Resource<'r>`: what the executor needs (thread pool, or `()` for zero-resource)
/// - `Session<'s>`: what runs the fold (borrows the resource)
/// - `attach()`: bind a resource to produce a session
/// - `with_session()`: create resource + attach + scope (the internal "do everything" path)
pub trait ExecutorSpec: Copy {
    type Resource<'r> where Self: 'r;
    type Session<'s>: 's where Self: 's;
    fn attach(self, resource: Self::Resource<'_>) -> Self::Session<'_>;
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

impl<D, S: Clone> Clone for Exec<D, S> {
    fn clone(&self) -> Self { Exec::new(self.0.clone()) }
}
impl<D, S: Copy> Copy for Exec<D, S> {}
// ANCHOR_END: exec_struct

/// Safe reinterpret: &T → &Exec<D, T> via repr(transparent).
fn wrap_ref<D, T>(inner: &T) -> &Exec<D, T> {
    // SAFETY: Exec is repr(transparent) over T. PhantomData<D> is ZST.
    unsafe { &*(inner as *const T as *const Exec<D, T>) }
}

// ── Block A: run (S implements Executor) ────────────
// Works for both Specs and Sessions.

impl<D, S> Exec<D, S> {
    // ANCHOR: inherent_run
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

// ── Block B: session + attach (S implements ExecutorSpec) ──
// Only on Spec-level Exec. Not available on Session-level Exec.

// ANCHOR: exec_session
impl<D, S: ExecutorSpec> Exec<D, S> {
    /// Enter the session scope. The resource is created, the session lives
    /// for the closure. Multiple `.run()` calls inside share the resource.
    pub fn session<R>(&self, f: impl for<'s> FnOnce(&Exec<D, S::Session<'s>>) -> R) -> R {
        self.0.with_session(|session| f(wrap_ref(session)))
    }

    /// Attach a pre-created resource, producing a session-level Exec.
    /// Consumes the Spec (partial application). The resource type is
    /// executor-specific (defined by the `Resource` GAT).
    pub fn attach(self, resource: S::Resource<'_>) -> Exec<D, S::Session<'_>> {
        Exec::new(self.0.attach(resource))
    }
}
// ANCHOR_END: exec_session

// ── Blanket: Exec implements Executor for generic code ──

impl<N: 'static, R: 'static, D: Domain<N>, S: Executor<N, R, D>> Executor<N, R, D> for Exec<D, S> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &D::Treeish, root: &N) -> R {
        Executor::<N, R, D>::run(&self.0, fold, graph, root)
    }
}

