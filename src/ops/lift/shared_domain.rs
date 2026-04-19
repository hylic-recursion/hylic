//! SharedDomainLift — bundle of Send+Sync+Clone+'static bounds for
//! lifts used in the Shared domain. The bare Lift is domain-neutral;
//! this marker carries the bounds the Shared-domain closure storage
//! needs. Blanket impl; no opt-in.

use super::core::Lift;

pub trait SharedDomainLift<N, H, R>:
    Lift<N, H, R> + Clone + Send + Sync + 'static
where
    N: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    Self::N2:   Clone + Send + Sync + 'static,
    Self::MapH: Clone + Send + Sync + 'static,
    Self::MapR: Clone + Send + Sync + 'static,
{}

impl<L, N, H, R> SharedDomainLift<N, H, R> for L
where
    L: Lift<N, H, R> + Clone + Send + Sync + 'static,
    N: Clone + Send + Sync + 'static,
    H: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
    L::N2:   Clone + Send + Sync + 'static,
    L::MapH: Clone + Send + Sync + 'static,
    L::MapR: Clone + Send + Sync + 'static,
{}
