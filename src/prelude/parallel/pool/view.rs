//! ViewHandle + PoolExecView: concrete bridge types connecting
//! base/WorkPool to the submit and fork-join abstractions.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use super::super::base::steal_queue::StealQueue;
use super::super::base::task_slot::TaskSlot;
use super::super::base::unsafe_core::task_ref::TaskRef;
use super::super::base::pool::{WakeSignal, WorkPool, steal_from_views};
use super::super::submit::{TaskSubmitter, TaskRunner};

// ── ViewHandle ───────────────────────────────────────

/// Shared state behind a ViewHandle. One Arc clone per handle clone
/// instead of three separate Arc clones (deque + signal + views).
struct ViewHandleInner {
    deque: Arc<StealQueue<TaskRef>>,
    signal: Arc<WakeSignal>,
    views: Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
}

#[derive(Clone)]
pub struct ViewHandle(Arc<ViewHandleInner>);

impl ViewHandle {
    fn new(
        deque: Arc<StealQueue<TaskRef>>,
        signal: Arc<WakeSignal>,
        views: Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
    ) -> Self {
        ViewHandle(Arc::new(ViewHandleInner { deque, signal, views }))
    }

    pub fn submit<F: FnOnce() + Send + 'static>(&self, f: F) {
        <Self as TaskSubmitter>::submit(self, f)
    }

    pub fn help_once(&self) -> bool {
        <Self as TaskSubmitter>::help_once(self)
    }
}

impl TaskSubmitter for ViewHandle {
    fn submit<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.0.deque.push(TaskRef::from_fn(f));
        self.0.signal.wake_one();
    }

    fn help_once(&self) -> bool {
        if let Some(task_ref) = self.0.deque.steal() {
            unsafe { task_ref.execute(); }
            return true;
        }
        if let Some(task_ref) = steal_from_views(&self.0.views) {
            unsafe { task_ref.execute(); }
            return true;
        }
        false
    }
}

// ── PoolExecView ─────────────────────────────────────

pub struct PoolExecView {
    deque: Arc<StealQueue<TaskRef>>,
    signal: Arc<WakeSignal>,
    views: Arc<Mutex<Vec<Arc<StealQueue<TaskRef>>>>>,
}

impl PoolExecView {
    pub fn new(pool: &WorkPool) -> Self {
        let deque = Arc::new(StealQueue::new());
        pool.views.lock().unwrap().push(deque.clone());
        PoolExecView {
            deque,
            signal: pool.signal.clone(),
            views: pool.views.clone(),
        }
    }

    /// Fork-join: push f2 as stack-allocated task, run f1, race on f2's slot.
    pub fn join<A: Send, B: Send>(
        &self,
        f1: impl FnOnce() -> A + Send,
        f2: impl FnOnce() -> B + Send,
    ) -> (A, B) {
        let slot = TaskSlot::new(f2);
        let pos = self.deque.push(slot.as_task_ref());
        self.signal.wake_one();

        let result_a = catch_unwind(AssertUnwindSafe(f1));

        if self.deque.try_reclaim(pos) {
            slot.run_locally();
        } else {
            while !slot.is_done() {
                if !self.help_once() {
                    std::hint::spin_loop();
                }
            }
        }

        let b = slot.take_result();
        match result_a {
            Ok(a) => (a, b),
            Err(e) => resume_unwind(e),
        }
    }

    pub fn handle(&self) -> ViewHandle {
        <Self as TaskRunner>::submitter(self)
    }

    pub fn help_once(&self) -> bool {
        <Self as TaskRunner>::help_once(self)
    }

    pub fn deque_len(&self) -> usize {
        self.deque.len()
    }

    pub fn views_count(&self) -> usize {
        self.views.lock().unwrap().len()
    }
}

impl TaskRunner for PoolExecView {
    type Submitter = ViewHandle;

    fn submitter(&self) -> ViewHandle {
        ViewHandle::new(
            self.deque.clone(),
            self.signal.clone(),
            self.views.clone(),
        )
    }

    fn help_once(&self) -> bool {
        if let Some(task) = self.deque.steal() {
            unsafe { task.execute(); }
            return true;
        }
        if let Some(task) = steal_from_views(&self.views) {
            unsafe { task.execute(); }
            return true;
        }
        false
    }
}

impl Drop for PoolExecView {
    fn drop(&mut self) {
        let ptr = Arc::as_ptr(&self.deque);
        let mut views = self.views.lock().unwrap();
        if let Some(pos) = views.iter().position(|d| Arc::as_ptr(d) == ptr) {
            views.swap_remove(pos);
        }
    }
}
