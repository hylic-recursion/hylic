//! EventCount: lock-free worker parking via atomic epoch + futex.
//!
//! Lost wakeup is structurally impossible: if a notification fires between
//! prepare() and wait(), the epoch has changed and wait returns immediately.
//!
//! Uses the `atomic-wait` crate (direct futex on Linux, cross-platform).

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Copy, Clone)]
pub(super) struct Token(u32);

impl Token {
    pub fn epoch(self) -> u32 { self.0 }
}

pub(super) struct EventCount {
    epoch: AtomicU32,
}

impl EventCount {
    pub fn new() -> Self { EventCount { epoch: AtomicU32::new(0) } }

    /// Snapshot the current epoch. Call BEFORE checking conditions.
    pub fn prepare(&self) -> Token {
        Token(self.epoch.load(Ordering::Acquire))
    }

    /// Sleep if epoch hasn't changed since prepare(). Returns immediately
    /// if a notification arrived between prepare() and wait().
    pub fn wait(&self, token: Token) {
        atomic_wait::wait(&self.epoch, token.0);
    }

    /// Wake one sleeping thread.
    pub fn notify_one(&self) {
        self.epoch.fetch_add(1, Ordering::Release);
        atomic_wait::wake_one(&self.epoch);
    }

    /// Wake all sleeping threads.
    pub fn notify_all(&self) {
        self.epoch.fetch_add(1, Ordering::Release);
        atomic_wait::wake_all(&self.epoch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32 as AU32};

    #[test]
    fn notify_before_wait_returns_immediately() {
        let ec = EventCount::new();
        let token = ec.prepare();
        ec.notify_one(); // epoch changes
        ec.wait(token);  // returns immediately — epoch != token
    }

    #[test]
    fn concurrent_notify_wake() {
        let ec = Arc::new(EventCount::new());
        let woke = Arc::new(AtomicBool::new(false));

        let ec2 = ec.clone();
        let woke2 = woke.clone();
        let handle = std::thread::spawn(move || {
            let token = ec2.prepare();
            ec2.wait(token);
            woke2.store(true, Ordering::Release);
        });

        std::thread::sleep(std::time::Duration::from_millis(10));
        ec.notify_one();
        handle.join().unwrap();
        assert!(woke.load(Ordering::Acquire));
    }

    #[test]
    fn shutdown_pattern() {
        let ec = Arc::new(EventCount::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        let ec2 = ec.clone();
        let sd2 = shutdown.clone();
        let handle = std::thread::spawn(move || {
            loop {
                let token = ec2.prepare();
                if sd2.load(Ordering::Acquire) { return; }
                ec2.wait(token);
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(5));
        shutdown.store(true, Ordering::Release);
        ec.notify_all();
        handle.join().unwrap();
    }

    #[test]
    fn stress_500_cycles() {
        let ec = Arc::new(EventCount::new());
        let counter = Arc::new(AU32::new(0));

        for _ in 0..500 {
            let ec2 = ec.clone();
            let c2 = counter.clone();
            let handle = std::thread::spawn(move || {
                let token = ec2.prepare();
                if c2.load(Ordering::Acquire) > 0 { return; }
                ec2.wait(token);
            });
            counter.store(1, Ordering::Release);
            ec.notify_all();
            handle.join().unwrap();
            counter.store(0, Ordering::Relaxed);
        }
    }
}
