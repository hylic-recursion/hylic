//! TreeishPipeline — the honest-base pipeline for users who have
//! a `Treeish<N>` directly (no `grow: Seed → N` step). Two base
//! slots: `treeish` and `fold`. `Self::Seed = ()` — no Seed
//! dispatch at the executor boundary; use `run_from_node`.

use crate::domain::Domain;

pub mod reshape;
pub mod transforms_shared;
pub mod transforms_local;
pub mod source_impl;

pub struct TreeishPipeline<D, N, H, R>
where D: Domain<N>,
      N: 'static, H: 'static, R: 'static,
{
    pub(crate) treeish: <D as Domain<N>>::Graph<N>,
    pub(crate) fold:    <D as Domain<N>>::Fold<H, R>,
}

impl<D, N, H, R> Clone for TreeishPipeline<D, N, H, R>
where D: Domain<N>,
      N: 'static, H: 'static, R: 'static,
      <D as Domain<N>>::Graph<N>:   Clone,
      <D as Domain<N>>::Fold<H, R>: Clone,
{
    fn clone(&self) -> Self {
        TreeishPipeline {
            treeish: self.treeish.clone(),
            fold:    self.fold.clone(),
        }
    }
}

impl<D, N, H, R> TreeishPipeline<D, N, H, R>
where D: Domain<N>,
      N: 'static, H: 'static, R: 'static,
{
    pub fn new_domain(
        treeish: <D as Domain<N>>::Graph<N>,
        fold:    <D as Domain<N>>::Fold<H, R>,
    ) -> Self {
        TreeishPipeline { treeish, fold }
    }
}

// ── Shared convenience constructor ─────────────────────

impl<N, H, R> TreeishPipeline<crate::domain::Shared, N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    /// Shared-specific constructor that takes a `Treeish<N>` and
    /// borrows a `Fold<N, H, R>`. Mirrors the pre-5/5 API.
    pub fn new(
        treeish: crate::graph::Treeish<N>,
        fold:    &crate::domain::shared::fold::Fold<N, H, R>,
    ) -> Self {
        TreeishPipeline { treeish, fold: fold.clone() }
    }
}

// ── Local convenience constructor ──────────────────────

impl<N, H, R> TreeishPipeline<crate::domain::Local, N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    /// Local-specific constructor using Rc-based storage. Non-Send
    /// folds/treeishes compose into a Local pipeline; sequential
    /// execution only (Fused).
    pub fn new_local(
        treeish: crate::domain::local::edgy::Edgy<N, N>,
        fold:    crate::domain::local::Fold<N, H, R>,
    ) -> Self {
        TreeishPipeline { treeish, fold }
    }
}
