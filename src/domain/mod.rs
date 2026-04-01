//! Boxing domains — how closures inside Fold/Treeish are stored.
//!
//! Each domain is a marker type implementing [`Domain`], providing
//! concrete Fold and Treeish types via GATs. Three built-in domains:
//!
//! | Domain | Storage | Clone | Send+Sync |
//! |--------|---------|-------|-----------|
//! | [`Shared`] | `Arc<dyn Fn + Send + Sync>` | yes | yes |
//! | [`Local`] | `Rc<dyn Fn>` | yes | no |
//! | [`Owned`] | `Box<dyn Fn>` | no | no |

pub mod shared;
pub mod local;
pub mod owned;

use crate::ops::{FoldOps, TreeOps};

/// A boxing domain: selects how closures are stored in Fold and Treeish.
///
/// Each domain provides concrete types via GATs. Executors are
/// parameterized by the domain, and each executor declares which
/// domains it supports.
pub trait Domain<N: 'static>: 'static {
    type Fold<H: 'static, R: 'static>: FoldOps<N, H, R>;
    type Treeish: TreeOps<N>;
}

/// Arc-based storage. Clone, Send+Sync. Required for Rayon, Lifts,
/// and pipeline composition (GraphWithFold).
pub struct Shared;

/// Rc-based storage. Clone, not Send+Sync. Lighter refcount than
/// Shared. Works with Fused and Sequential.
pub struct Local;

/// Box-based storage. Not Clone. Lightest — no refcount. Works
/// with Fused only (no cloning needed for fused recursion).
pub struct Owned;

impl<N: 'static> Domain<N> for Shared {
    type Fold<H: 'static, R: 'static> = crate::fold::Fold<N, H, R>;
    type Treeish = crate::graph::Treeish<N>;
}

impl<N: 'static> Domain<N> for Local {
    type Fold<H: 'static, R: 'static> = local::Fold<N, H, R>;
    type Treeish = local::Treeish<N>;
}

impl<N: 'static> Domain<N> for Owned {
    type Fold<H: 'static, R: 'static> = owned::Fold<N, H, R>;
    type Treeish = owned::Treeish<N>;
}
