//! Completion<R>: one-shot result slot with parent notification.
//!
//! Used by ParEager's continuation-passing. When set(value) is called:
//! 1. The result is stored
//! 2. If a parent callback was attached, it fires with the value

use std::sync::{Arc, Mutex};
use super::submit::TaskSubmitter;

struct CompletionInner<R> {
    result: Mutex<Option<R>>,
    parent: Mutex<Option<Box<dyn FnOnce(R) + Send>>>,
}

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

    pub(crate) fn set(&self, value: R) {
        let callback = {
            let mut result = self.inner.result.lock().unwrap();
            *result = Some(value.clone());
            self.inner.parent.lock().unwrap().take()
        };
        if let Some(cb) = callback { cb(value); }
    }

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

    pub(crate) fn get(&self) -> Option<R> {
        self.inner.result.lock().unwrap().clone()
    }

    pub(crate) fn wait(&self, handle: impl TaskSubmitter) -> R {
        loop {
            if let Some(r) = self.get() { return r; }
            if !handle.help_once() { std::hint::spin_loop(); }
        }
    }
}
