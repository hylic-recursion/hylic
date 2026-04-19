//! SharedDomainLift — bundle of Send+Sync bounds for pipelines using
//! the Shared domain. The bare `Lift` trait is domain-neutral; this
//! trait adds the bounds the Shared-domain closure storage imposes.
//!
//! Blanket impl: any Lift whose inputs and outputs are Send+Sync is
//! automatically a SharedDomainLift. Downstream impl blocks use one
//! `where` clause instead of spelling eight bounds.

use super::lift::Lift;

pub trait SharedDomainLift<N, Seed, H, R>: Lift<N, Seed, H, R> + Clone + Send + Sync + 'static
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    Self::N2: Clone + Send + Sync + 'static,
    Self::Seed2: Clone + Send + Sync + 'static,
    Self::MapH: Clone + Send + Sync + 'static,
    Self::MapR: Clone + Send + Sync + 'static,
{}

impl<L, N, Seed, H, R> SharedDomainLift<N, Seed, H, R> for L
where
    L: Lift<N, Seed, H, R> + Clone + Send + Sync + 'static,
    N: Clone + Send + Sync + 'static,
    Seed: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    L::N2: Clone + Send + Sync + 'static,
    L::Seed2: Clone + Send + Sync + 'static,
    L::MapH: Clone + Send + Sync + 'static,
    L::MapR: Clone + Send + Sync + 'static,
{}
