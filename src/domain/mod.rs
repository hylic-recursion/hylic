//! Boxing domains — how closures inside Fold/Treeish are stored.
//!
//! Each domain is a marker type implementing [`Domain`], providing
//! concrete Fold, Graph, and Grow types via GATs. Three built-in
//! domains:
//!
//! | Domain | Storage | Clone | Send+Sync |
//! |--------|---------|-------|-----------|
//! | [`Shared`] | `Arc<dyn Fn + Send + Sync>` | yes | yes |
//! | [`Local`] | `Rc<dyn Fn>` | yes | no |
//! | [`Owned`] | `Box<dyn Fn>` | no | no |

#![allow(missing_docs)] // implementation surface; items documented at the trait/type they implement

pub mod shared;
pub mod local;
pub mod owned;
pub(crate) mod fold_combinators;

use std::sync::Arc;
use std::rc::Rc;
use crate::ops::FoldOps;
use crate::graph::{self, Edgy};
use crate::domain::local::edgy as local_edgy;
use crate::domain::owned::edgy as owned_edgy;

/// A boxing domain: selects how fold/grow/graph closures are stored.
///
/// Under (a-uniform), `make_fold` / `make_grow` / `make_graph`
/// declare uniform `Send + Sync + 'static` bounds on the closure
/// argument, matching Shared's requirement. Local and Owned
/// implementations satisfy the bound at input time but shed
/// Send+Sync at storage time (Rc / Box don't carry those).
///
/// Per-domain `FoldTransformsByRef` / `FoldTransformsByValue`
/// (phase 5/1) and a future `GraphTransforms` hierarchy carry
/// per-domain bounds naturally; `make_*` here is the uniform
/// trait-level constructor used by domain-generic Lift bodies.
// ANCHOR: domain_trait
pub trait Domain<N: 'static>: 'static {
    type Fold<H: 'static, R: 'static>: FoldOps<N, H, R>;
    type Graph<E: 'static> where E: 'static;
    type Grow<Seed: 'static, NOut: 'static>;

    /// Construct a fold from three closures. Uniform Send+Sync
    /// bound; each domain sheds Send+Sync at storage time if it
    /// doesn't need it.
    fn make_fold<H: 'static, R: 'static>(
        init: impl Fn(&N) -> H + Send + Sync + 'static,
        acc:  impl Fn(&mut H, &R) + Send + Sync + 'static,
        fin:  impl Fn(&H) -> R + Send + Sync + 'static,
    ) -> Self::Fold<H, R>;

    /// Construct a grow closure from a Fn. Uniform Send+Sync bound.
    fn make_grow<Seed: 'static, NOut: 'static>(
        f: impl Fn(&Seed) -> NOut + Send + Sync + 'static,
    ) -> Self::Grow<Seed, NOut>;

    /// Invoke a stored grow closure.
    fn invoke_grow<Seed: 'static, NOut: 'static>(
        g: &Self::Grow<Seed, NOut>,
        s: &Seed,
    ) -> NOut;

    /// Construct a graph (Edgy) closure. Uniform Send+Sync bound.
    fn make_graph<E: 'static>(
        visit: impl Fn(&N, &mut dyn FnMut(&E)) + Send + Sync + 'static,
    ) -> Self::Graph<E>;
}
// ANCHOR_END: domain_trait

/// Arc-based storage. Clone, Send+Sync. Required for parallel
/// executors (Funnel) and pipeline composition.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shared;

/// Rc-based storage. Clone, not Send+Sync. Lighter refcount than
/// Shared. Works with Fused only.
#[derive(Clone, Copy, Debug, Default)]
pub struct Local;

/// Box-based storage. Not Clone. Lightest — no refcount. Works
/// with Fused only. Single-use semantics under Phase-5 pipelines.
#[derive(Clone, Copy, Debug, Default)]
pub struct Owned;

// ── Shared impl ────────────────────────────────────

impl<N: 'static> Domain<N> for Shared {
    type Fold<H: 'static, R: 'static> = shared::fold::Fold<N, H, R>;
    type Graph<E: 'static> = Edgy<N, E> where E: 'static;
    type Grow<Seed: 'static, NOut: 'static> = Arc<dyn Fn(&Seed) -> NOut + Send + Sync>;

    fn make_fold<H: 'static, R: 'static>(
        init: impl Fn(&N) -> H + Send + Sync + 'static,
        acc:  impl Fn(&mut H, &R) + Send + Sync + 'static,
        fin:  impl Fn(&H) -> R + Send + Sync + 'static,
    ) -> shared::fold::Fold<N, H, R> {
        shared::fold::Fold::new(init, acc, fin)
    }

    fn make_grow<Seed: 'static, NOut: 'static>(
        f: impl Fn(&Seed) -> NOut + Send + Sync + 'static,
    ) -> Arc<dyn Fn(&Seed) -> NOut + Send + Sync> {
        Arc::new(f)
    }

    fn invoke_grow<Seed: 'static, NOut: 'static>(
        g: &Arc<dyn Fn(&Seed) -> NOut + Send + Sync>,
        s: &Seed,
    ) -> NOut {
        (g)(s)
    }

    fn make_graph<E: 'static>(
        visit: impl Fn(&N, &mut dyn FnMut(&E)) + Send + Sync + 'static,
    ) -> Edgy<N, E> {
        graph::edgy_visit(visit)
    }
}

// ── Local impl ─────────────────────────────────────
// NOTE: Local's Grow uses Rc<dyn Fn> (no Send+Sync at storage),
// but make_grow still demands Send+Sync on input per (a-uniform).
// Graph uses the shared Arc-based Edgy — per-domain Edgy is a
// deferred enhancement; Graph-side closures on Local currently
// require Send+Sync.

impl<N: 'static> Domain<N> for Local {
    type Fold<H: 'static, R: 'static> = local::Fold<N, H, R>;
    type Graph<E: 'static> = local_edgy::Edgy<N, E> where E: 'static;
    type Grow<Seed: 'static, NOut: 'static> = Rc<dyn Fn(&Seed) -> NOut>;

    fn make_fold<H: 'static, R: 'static>(
        init: impl Fn(&N) -> H + Send + Sync + 'static,
        acc:  impl Fn(&mut H, &R) + Send + Sync + 'static,
        fin:  impl Fn(&H) -> R + Send + Sync + 'static,
    ) -> local::Fold<N, H, R> {
        local::Fold::new(init, acc, fin)
    }

    fn make_grow<Seed: 'static, NOut: 'static>(
        f: impl Fn(&Seed) -> NOut + Send + Sync + 'static,
    ) -> Rc<dyn Fn(&Seed) -> NOut> {
        Rc::new(f)
    }

    fn invoke_grow<Seed: 'static, NOut: 'static>(
        g: &Rc<dyn Fn(&Seed) -> NOut>,
        s: &Seed,
    ) -> NOut {
        (g)(s)
    }

    fn make_graph<E: 'static>(
        visit: impl Fn(&N, &mut dyn FnMut(&E)) + Send + Sync + 'static,
    ) -> local_edgy::Edgy<N, E> {
        local_edgy::edgy_visit(visit)
    }
}

// ── Owned impl ─────────────────────────────────────

impl<N: 'static> Domain<N> for Owned {
    type Fold<H: 'static, R: 'static> = owned::Fold<N, H, R>;
    type Graph<E: 'static> = owned_edgy::Edgy<N, E> where E: 'static;
    type Grow<Seed: 'static, NOut: 'static> = Box<dyn Fn(&Seed) -> NOut>;

    fn make_fold<H: 'static, R: 'static>(
        init: impl Fn(&N) -> H + Send + Sync + 'static,
        acc:  impl Fn(&mut H, &R) + Send + Sync + 'static,
        fin:  impl Fn(&H) -> R + Send + Sync + 'static,
    ) -> owned::Fold<N, H, R> {
        owned::Fold::new(init, acc, fin)
    }

    fn make_grow<Seed: 'static, NOut: 'static>(
        f: impl Fn(&Seed) -> NOut + Send + Sync + 'static,
    ) -> Box<dyn Fn(&Seed) -> NOut> {
        Box::new(f)
    }

    fn invoke_grow<Seed: 'static, NOut: 'static>(
        g: &Box<dyn Fn(&Seed) -> NOut>,
        s: &Seed,
    ) -> NOut {
        (g)(s)
    }

    fn make_graph<E: 'static>(
        visit: impl Fn(&N, &mut dyn FnMut(&E)) + Send + Sync + 'static,
    ) -> owned_edgy::Edgy<N, E> {
        owned_edgy::edgy_visit(visit)
    }
}

