//! SharedDomainLift — marker trait bundling the Send+Sync+Clone+'static
//! bounds that the Shared-domain closure storage imposes.
//!
//! The bare `Lift` trait is domain-neutral: no Send/Sync. When a
//! pipeline actually USES the Shared domain (whose Fold/Edgy closures
//! are stored in `Arc<dyn Fn + Send + Sync + 'static>`), every
//! value that crosses or is captured by those closures must be
//! Send + Sync.
//!
//! This trait encodes that bundle once. The blanket impl makes any
//! Lift whose inputs and GAT projections satisfy the bounds
//! automatically a `SharedDomainLift`. Downstream impl blocks cite
//! one line (`where L: SharedDomainLift<N, Seed, H, R>`) instead of
//! spelling the four GAT-projection bounds repeatedly.

use super::lift::Lift;

pub trait SharedDomainLift<N, Seed, H, R>: Lift + Clone + Send + Sync + 'static
where
    N: Clone + Send + Sync + 'static,
    Seed: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    Self::N2<N>: Clone + Send + Sync + 'static,
    Self::Seed2<Seed>: Clone + Send + Sync + 'static,
    Self::MapH<N, H, R>: Clone + Send + Sync + 'static,
    Self::MapR<N, H, R>: Clone + Send + Sync + 'static,
{}

impl<L, N, Seed, H, R> SharedDomainLift<N, Seed, H, R> for L
where
    L: Lift + Clone + Send + Sync + 'static,
    N: Clone + Send + Sync + 'static,
    Seed: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    L::N2<N>: Clone + Send + Sync + 'static,
    L::Seed2<Seed>: Clone + Send + Sync + 'static,
    L::MapH<N, H, R>: Clone + Send + Sync + 'static,
    L::MapR<N, H, R>: Clone + Send + Sync + 'static,
{}
