//! Unsafe Send/Sync primitives for parallel lifts.
//!
//! All `unsafe impl Send/Sync` assertions live in this module.
//! The rest of the parallel codebase (lazy.rs, eager.rs, completion.rs)
//! is safe Rust — it calls into these primitives at well-defined
//! boundaries with documented safety invariants.

use crate::ops::FoldOps;

// ── SyncRef: scoped-thread borrow ────────────────────

/// A reference safe to share across scoped threads.
///
/// # Safety invariant
/// The pointee outlives all users. Guaranteed by `WorkPool::with`
/// (std::thread::scope — all workers join before the scope exits).
/// Workers only deref + call (read-only). No Rc cloning, no mutation
/// of refcounts.
pub struct SyncRef<'a, T: ?Sized>(pub &'a T);

unsafe impl<T: ?Sized> Sync for SyncRef<'_, T> {}
unsafe impl<T: ?Sized> Send for SyncRef<'_, T> {}

impl<T: ?Sized> std::ops::Deref for SyncRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.0 }
}

// ── FoldPtr: lifetime-erased fold for continuation-passing ──

/// Raw pointer to a fold's operations (trait object). Lifetime-erased.
///
/// # Safety invariant
/// The pointee (a `D::Fold<H, R>` stored in the Lift's stash) outlives
/// all tasks that hold a FoldPtr. Guaranteed by: unwrap waits for root
/// Completion before taking the fold from the stash. Workers only call
/// accumulate/finalize through the pointer — no Rc clone, no refcount
/// mutation.
///
/// Created via `FoldPtr::from_ref()` during lift_fold. Copied into
/// Collectors and leaf task closures. Used during Phase 2 execution.
pub(crate) struct FoldPtr<N, H, R> {
    ptr: *const dyn FoldOps<N, H, R>,
}

unsafe impl<N, H, R> Send for FoldPtr<N, H, R> {}
unsafe impl<N, H, R> Sync for FoldPtr<N, H, R> {}

impl<N, H, R> Clone for FoldPtr<N, H, R> {
    fn clone(&self) -> Self { *self }
}
impl<N, H, R> Copy for FoldPtr<N, H, R> {}

impl<N, H, R> FoldPtr<N, H, R> {
    /// Create from a trait-object reference. Caller guarantees the
    /// pointee outlives all uses of the returned FoldPtr.
    ///
    /// # Safety
    /// The referenced fold must remain alive and at a stable address
    /// for the entire duration that any copy of this FoldPtr exists.
    pub(crate) unsafe fn from_ref<F: FoldOps<N, H, R> + 'static>(fold: &F) -> Self {
        FoldPtr {
            ptr: fold as &dyn FoldOps<N, H, R> as *const dyn FoldOps<N, H, R>,
        }
    }

    /// Call accumulate through the pointer.
    ///
    /// # Safety
    /// The pointee must still be alive.
    pub(crate) unsafe fn accumulate(&self, heap: &mut H, result: &R) {
        unsafe { (*self.ptr).accumulate(heap, result) };
    }

    /// Call finalize through the pointer.
    ///
    /// # Safety
    /// The pointee must still be alive.
    pub(crate) unsafe fn finalize(&self, heap: &H) -> R {
        unsafe { (*self.ptr).finalize(heap) }
    }
}

// ── AssertSend: ConstructFold bridge ─────────────────

/// Wrapper asserting Send+Sync for values known to satisfy these
/// bounds by the caller's safety invariant.
///
/// # Safety invariant
/// Used only in `ConstructFold<Shared>` where the closures actually
/// ARE Send+Sync (they capture domain-Shared data which is Arc-based).
/// The compiler can't deduce Send+Sync from the trait signature, so
/// this wrapper bridges the gap.
///
/// # Capture pattern
/// Use `.get()` (method call) instead of `.0` (field access) to
/// force Rust 2021 precise captures to grab the whole AssertSend
/// wrapper (which IS Send+Sync), not just the inner field (which
/// the compiler thinks might not be).
pub(crate) struct AssertSend<T>(pub(crate) T);

unsafe impl<T> Send for AssertSend<T> {}
unsafe impl<T> Sync for AssertSend<T> {}

impl<T> AssertSend<T> {
    pub(crate) fn get(&self) -> &T { &self.0 }
}
