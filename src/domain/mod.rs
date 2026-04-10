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
// ANCHOR: domain_trait
pub trait Domain<N: 'static>: 'static {
    type Fold<H: 'static, R: 'static>: FoldOps<N, H, R>;
    type Treeish: TreeOps<N>;
}
// ANCHOR_END: domain_trait

/// Arc-based storage. Clone, Send+Sync. Required for parallel
/// executors (Funnel) and pipeline composition (GraphWithFold).
pub struct Shared;

/// Rc-based storage. Clone, not Send+Sync. Lighter refcount than
/// Shared. Works with Fused.
pub struct Local;

/// Box-based storage. Not Clone. Lightest — no refcount. Works
/// with Fused only (no cloning needed for fused recursion).
pub struct Owned;

/// Construct a domain fold from three unboxed closures.
///
/// Used by lift machinery (`hylic-parallel-lifts`) to construct folds
/// in the target domain without knowing the concrete Fold type.
///
/// # Safety
///
/// The closures' trait bounds are intentionally weaker than required
/// by the domain's storage. For the Shared domain, the concrete impl
/// wraps closures in `Arc<dyn Fn + Send + Sync>` — callers MUST
/// ensure the closures actually satisfy Send + Sync. Passing
/// non-Send closures to the Shared impl is undefined behavior.
///
/// The Local and Owned impls have no additional safety requirements
/// beyond the `'static` bounds.
pub trait ConstructFold<N: 'static>: Domain<N> {
    /// # Safety
    /// For Shared: closures must be Send+Sync (stored in Arc).
    unsafe fn make_fold<H: 'static, R: 'static>(
        init: impl Fn(&N) -> H + 'static,
        acc: impl Fn(&mut H, &R) + 'static,
        fin: impl Fn(&H) -> R + 'static,
    ) -> Self::Fold<H, R>;
}

impl<N: 'static> Domain<N> for Shared {
    type Fold<H: 'static, R: 'static> = shared::fold::Fold<N, H, R>;
    type Treeish = shared::graph::Treeish<N>;
}

impl<N: 'static> Domain<N> for Local {
    type Fold<H: 'static, R: 'static> = local::Fold<N, H, R>;
    type Treeish = local::Treeish<N>;
}

impl<N: 'static> Domain<N> for Owned {
    type Fold<H: 'static, R: 'static> = owned::Fold<N, H, R>;
    type Treeish = owned::Treeish<N>;
}

impl<N: 'static> ConstructFold<N> for Shared {
    unsafe fn make_fold<H: 'static, R: 'static>(
        init: impl Fn(&N) -> H + 'static,
        acc: impl Fn(&mut H, &R) + 'static,
        fin: impl Fn(&H) -> R + 'static,
    ) -> shared::fold::Fold<N, H, R> {
        // SAFETY: caller guarantees closures are Send+Sync.
        // AssertSend bridges the type gap. Method call (.get())
        // forces Rust 2021 precise captures to grab the whole wrapper.

        /// Wrapper asserting Send+Sync for values known to satisfy these
        /// bounds by the caller's safety contract. Use `.get()` (method call)
        /// to force Rust 2021 precise captures to grab the whole wrapper.
        struct AssertSend<T>(T);
        unsafe impl<T> Send for AssertSend<T> {}
        unsafe impl<T> Sync for AssertSend<T> {}
        impl<T> AssertSend<T> { fn get(&self) -> &T { &self.0 } }
        let init = AssertSend(init);
        let acc = AssertSend(acc);
        let fin = AssertSend(fin);
        shared::fold::fold(
            move |n: &N| init.get()(n),
            move |h: &mut H, r: &R| acc.get()(h, r),
            move |h: &H| fin.get()(h),
        )
    }
}

impl<N: 'static> ConstructFold<N> for Local {
    unsafe fn make_fold<H: 'static, R: 'static>(
        init: impl Fn(&N) -> H + 'static,
        acc: impl Fn(&mut H, &R) + 'static,
        fin: impl Fn(&H) -> R + 'static,
    ) -> local::Fold<N, H, R> {
        // No Send+Sync needed — Rc storage. Safe for all closures.
        local::fold(init, acc, fin)
    }
}
