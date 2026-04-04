//! FunnelPool: dedicated thread pool for the hylo-funnel executor.
//!
//! Self-contained: own MPMC queue, own eventcount, own threads.
//! No Mutex, no Arc<StealQueue>, no views Vec.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::mpmc::BoundedMpmc;
use super::eventcount::EventCount;
use super::task_ref::TaskRef;

const DEFAULT_QUEUE_CAPACITY: usize = 4096;

pub(super) struct FunnelPoolSpec {
    pub n_workers: usize,
}

impl FunnelPoolSpec {
    pub fn threads(n: usize) -> Self { FunnelPoolSpec { n_workers: n } }
}

pub(super) struct FunnelPool {
    queue: Arc<BoundedMpmc<TaskRef>>,
    event: Arc<EventCount>,
    shutdown: AtomicBool,
}

impl FunnelPool {
    pub fn with<R>(spec: FunnelPoolSpec, f: impl FnOnce(&Arc<Self>) -> R) -> R {
        let pool = Arc::new(FunnelPool {
            queue: Arc::new(BoundedMpmc::new(DEFAULT_QUEUE_CAPACITY)),
            event: Arc::new(EventCount::new()),
            shutdown: AtomicBool::new(false),
        });
        std::thread::scope(|s| {
            for _ in 0..spec.n_workers {
                let queue = pool.queue.clone();
                let event = pool.event.clone();
                let shutdown = &pool.shutdown;
                s.spawn(move || worker_loop(&queue, &event, shutdown));
            }
            struct ShutdownGuard<'a>(&'a FunnelPool);
            impl Drop for ShutdownGuard<'_> {
                fn drop(&mut self) {
                    self.0.shutdown.store(true, Ordering::Release);
                    self.0.event.notify_all();
                }
            }
            let _guard = ShutdownGuard(&pool);
            f(&pool)
        })
    }

    pub fn submit<F: FnOnce() + Send + 'static>(&self, f: F) {
        let mut task = TaskRef::from_fn(f);
        loop {
            match self.queue.push(task) {
                Ok(()) => break,
                Err(returned) => {
                    task = returned;
                    // Queue full — backpressure: execute one task to free a slot
                    if let Some(t) = self.queue.pop() {
                        unsafe { t.execute(); }
                    } else {
                        std::hint::spin_loop();
                    }
                }
            }
        }
        self.event.notify_one();
    }

    pub fn help_once(&self) -> bool {
        if let Some(task) = self.queue.pop() {
            unsafe { task.execute(); }
            true
        } else {
            false
        }
    }
}

fn worker_loop(
    queue: &BoundedMpmc<TaskRef>,
    event: &EventCount,
    shutdown: &AtomicBool,
) {
    loop {
        if let Some(task) = queue.pop() {
            unsafe { task.execute(); }
            continue;
        }
        let token = event.prepare();
        if shutdown.load(Ordering::Acquire) { return; }
        if let Some(task) = queue.pop() {
            unsafe { task.execute(); }
            continue;
        }
        event.wait(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn basic_submit_and_help() {
        FunnelPool::with(FunnelPoolSpec::threads(2), |pool| {
            let done = Arc::new(AtomicBool::new(false));
            let d2 = done.clone();
            pool.submit(move || { d2.store(true, Ordering::Release); });
            while !done.load(Ordering::Acquire) {
                if !pool.help_once() { std::hint::spin_loop(); }
            }
        });
    }

    #[test]
    fn zero_workers() {
        FunnelPool::with(FunnelPoolSpec::threads(0), |pool| {
            let counter = Arc::new(AtomicU64::new(0));
            for _ in 0..10 {
                let c = counter.clone();
                pool.submit(move || { c.fetch_add(1, Ordering::Relaxed); });
            }
            while counter.load(Ordering::Relaxed) < 10 {
                if !pool.help_once() { std::hint::spin_loop(); }
            }
        });
    }

    #[test]
    fn lifecycle_stress_500() {
        for _ in 0..500 {
            FunnelPool::with(FunnelPoolSpec::threads(4), |_pool| {});
        }
    }

    #[test]
    fn lifecycle_with_work_500() {
        for _ in 0..500 {
            FunnelPool::with(FunnelPoolSpec::threads(4), |pool| {
                let done = Arc::new(AtomicBool::new(false));
                let d2 = done.clone();
                pool.submit(move || { d2.store(true, Ordering::Release); });
                while !done.load(Ordering::Acquire) {
                    if !pool.help_once() { std::hint::spin_loop(); }
                }
            });
        }
    }
}
