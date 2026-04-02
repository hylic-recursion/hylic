//! Completion<R>: one-shot result slot with parent notification.
//!
//! Pure safe synchronization primitive. Used by ParEager's
//! continuation-passing for Phase 2 result propagation.

use std::sync::{Arc, Mutex};

// ── CompletionInner ──────────────────────────────────

struct CompletionInner<R> {
    result: Mutex<Option<R>>,
    /// Type-erased parent callback. Set by attach_parent().
    /// Captures the parent Collector + child index, erasing H.
    parent: Mutex<Option<Box<dyn FnOnce(R) + Send>>>,
}

// ── Completion ───────────────────────────────────────

/// One-shot result slot with optional parent notification.
///
/// When `set(value)` is called:
/// 1. The result is stored
/// 2. If a parent callback was attached, it fires with the value
///
/// Lock ordering: result first, then parent (both in set and attach_parent).
pub(crate) struct Completion<R> {
    inner: Arc<CompletionInner<R>>,
}

impl<R> Clone for Completion<R> {
    fn clone(&self) -> Self { Completion { inner: self.inner.clone() } }
}

impl<R: Clone + Send + 'static> Completion<R> {
    pub(crate) fn new() -> Self {
        Completion { inner: Arc::new(CompletionInner {
            result: Mutex::new(None),
            parent: Mutex::new(None),
        })}
    }

    /// Store the result and fire the parent callback if attached.
    pub(crate) fn set(&self, value: R) {
        let callback = {
            let mut result = self.inner.result.lock().unwrap();
            *result = Some(value.clone());
            self.inner.parent.lock().unwrap().take()
        };
        if let Some(cb) = callback {
            cb(value);
        }
    }

    /// Attach a parent notification callback. If the result is already
    /// set, fires immediately (inline on the caller's thread).
    ///
    /// Lock ordering: result first, then parent — same as set().
    pub(crate) fn attach_parent(&self, callback: Box<dyn FnOnce(R) + Send>) {
        let result_guard = self.inner.result.lock().unwrap();
        if let Some(ref r) = *result_guard {
            let r = r.clone();
            drop(result_guard);
            callback(r);
        } else {
            let mut parent_guard = self.inner.parent.lock().unwrap();
            *parent_guard = Some(callback);
            drop(parent_guard);
            drop(result_guard);
        }
    }

    /// Non-blocking get.
    pub(crate) fn get(&self) -> Option<R> {
        self.inner.result.lock().unwrap().clone()
    }

    /// Block until result is ready, helping the pool while waiting.
    pub(crate) fn wait(&self, pool: &super::pool::WorkPool) -> R {
        loop {
            if let Some(r) = self.get() { return r; }
            if !pool.try_run_one() { std::hint::spin_loop(); }
        }
    }
}
