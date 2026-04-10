//! Executor: the strategy for recursive tree computation.
//!
//! `Exec<D, S>` is the sole user-facing interface. D is the domain
//! (determines fold type). S is the executor strategy. The graph type
//! G is inferred at the call site — any `TreeOps<N>` implementation works.
//!
//! ```ignore
//! use hylic::domain::shared as dom;
//! use hylic::graph;
//!
//! // One-shot
//! dom::exec(funnel::Spec::default(8)).run(&fold, &treeish, &root);
//!
//! // Session scope
//! dom::exec(funnel::Spec::default(8)).session(|s| {
//!     s.run(&fold, &treeish, &root);
//! });
//!
//! // Explicit attach
//! funnel::Pool::with(8, |pool| {
//!     dom::exec(funnel::Spec::default(8)).attach(pool).run(&fold, &treeish, &root);
//! });
//! ```

pub mod variant;

pub use variant::fused;
pub use variant::funnel;

use std::marker::PhantomData;
use crate::domain::Domain;
use crate::ops::TreeOps;

// ── Core trait: computation ─────────────────────────

// ANCHOR: executor_trait
/// Run a fold on a tree. Both Specs and Sessions implement this.
///
/// The fold is domain-specific (`D::Fold<H, R>`). The graph type G
/// is a trait-level parameter — each executor impl declares its own
/// bounds on G (e.g. Fused accepts any TreeOps, Funnel requires
/// Send+Sync). The compiler checks G at the call site.
pub trait Executor<N: 'static, R: 'static, D: Domain<N>, G: TreeOps<N> + 'static> {
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &G, root: &N) -> R;
}
// ANCHOR_END: executor_trait

// ── Core trait: lifecycle ───────────────────────────

// ANCHOR: executor_spec
/// Lifecycle: resource management + session creation.
/// Only Specs implement this. Sessions are the output.
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
    pub fn into_inner(self) -> S { self.0 }
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

impl<D, S> Exec<D, S> {
    // ANCHOR: inherent_run
    pub fn run<N: 'static, H: 'static, R: 'static, G: TreeOps<N> + 'static>(
        &self, fold: &<D as Domain<N>>::Fold<H, R>, graph: &G, root: &N,
    ) -> R
    where D: Domain<N>, S: Executor<N, R, D, G>
    {
        Executor::<N, R, D, G>::run(&self.0, fold, graph, root)
    }
    // ANCHOR_END: inherent_run
}

// ── Block B: session + attach (S implements ExecutorSpec) ──

// ANCHOR: exec_session
impl<D, S: ExecutorSpec> Exec<D, S> {
    pub fn session<R>(&self, f: impl for<'s> FnOnce(&Exec<D, S::Session<'s>>) -> R) -> R {
        self.0.with_session(|session| f(wrap_ref(session)))
    }

    pub fn attach(self, resource: S::Resource<'_>) -> Exec<D, S::Session<'_>> {
        Exec::new(self.0.attach(resource))
    }
}
// ANCHOR_END: exec_session

// ── Blanket: Exec implements Executor for generic code ──

impl<N: 'static, R: 'static, D: Domain<N>, G: TreeOps<N> + 'static, S: Executor<N, R, D, G>>
    Executor<N, R, D, G> for Exec<D, S>
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &G, root: &N) -> R {
        Executor::<N, R, D, G>::run(&self.0, fold, graph, root)
    }
}

// ── Blanket: &S implements Executor when S does ──
// Enables wrapping borrowed sessions in adapters (e.g., SeedAdapter<&S>).

impl<N: 'static, R: 'static, D: Domain<N>, G: TreeOps<N> + 'static, S: Executor<N, R, D, G>>
    Executor<N, R, D, G> for &S
{
    fn run<H: 'static>(&self, fold: &D::Fold<H, R>, graph: &G, root: &N) -> R {
        (**self).run(fold, graph, root)
    }
}
