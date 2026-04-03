//! StealQueue<T>: segmented monotonic push+steal queue.
//!
//! Safe API over unsafe_core. No pop. No resize. Monotonic indices.
//!
//! - `push(item) → u64`: claim position, write, publish. Returns position.
//! - `steal() → Option<T>`: advance top, CAS slot AVAILABLE → STOLEN, read.
//! - `try_reclaim(pos) → bool`: CAS slot AVAILABLE → RECLAIMED.
//!   If won, no worker will ever touch the value at this position.

use std::sync::atomic::{AtomicU64, Ordering, fence};
use super::unsafe_core::segment::SegmentTable;
use super::unsafe_core::slot;

pub struct StealQueue<T> {
    segments: SegmentTable<T>,
    bottom: CachePad<AtomicU64>,
    top: CachePad<AtomicU64>,
}

impl<T> StealQueue<T> {
    pub fn new() -> Self {
        StealQueue {
            segments: SegmentTable::new(),
            bottom: CachePad(AtomicU64::new(0)),
            top: CachePad(AtomicU64::new(0)),
        }
    }

    /// Push an item. Returns the position (for reclaim).
    pub fn push(&self, item: T) -> u64 {
        let pos = self.bottom.0.fetch_add(1, Ordering::Relaxed);
        let s = self.segments.get_slot(pos);
        unsafe { s.write(item); }
        pos
    }

    /// Steal the oldest available item (FIFO from top).
    pub fn steal(&self) -> Option<T> {
        loop {
            let t = self.top.0.load(Ordering::Acquire);
            fence(Ordering::SeqCst);
            let b = self.bottom.0.load(Ordering::Acquire);

            if b <= t { return None; }

            let s = self.segments.get_slot(t);
            let state = s.state();

            match state {
                slot::EMPTY => {
                    // Producer still writing. Yield and retry.
                    std::thread::yield_now();
                    continue;
                }
                slot::RECLAIMED => {
                    // Publisher reclaimed. Advance top past this dead slot.
                    let _ = self.top.0.compare_exchange_weak(
                        t, t + 1, Ordering::SeqCst, Ordering::Relaxed,
                    );
                    continue;
                }
                slot::STOLEN => {
                    // Already stolen by another worker. Advance top.
                    let _ = self.top.0.compare_exchange_weak(
                        t, t + 1, Ordering::SeqCst, Ordering::Relaxed,
                    );
                    continue;
                }
                slot::AVAILABLE => {
                    // Try to advance top AND steal the slot.
                    if self.top.0.compare_exchange_weak(
                        t, t + 1, Ordering::SeqCst, Ordering::Relaxed,
                    ).is_err() {
                        continue;
                    }
                    // Position claimed. Now CAS the slot.
                    if s.try_steal() {
                        return Some(unsafe { s.read() });
                    }
                    // Publisher reclaimed between our state check and CAS.
                    // Position consumed, no value. Continue.
                }
                _ => unreachable!(),
            }
        }
    }

    /// Try to reclaim the item at position `pos`.
    /// CAS slot AVAILABLE → RECLAIMED. If won, no worker will ever
    /// dereference the value. The publisher can safely drop the frame.
    ///
    /// Returns true if reclaimed (publisher won the race).
    /// Returns false if a worker already stole it.
    pub fn try_reclaim(&self, pos: u64) -> bool {
        self.segments.get_slot(pos).try_reclaim()
    }

    pub fn len(&self) -> usize {
        let t = self.top.0.load(Ordering::Acquire);
        let b = self.bottom.0.load(Ordering::Acquire);
        b.saturating_sub(t) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[repr(align(128))]
struct CachePad<T>(T);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn push_steal_basic() {
        let q = StealQueue::new();
        q.push(1); q.push(2); q.push(3);
        assert_eq!(q.steal(), Some(1));
        assert_eq!(q.steal(), Some(2));
        assert_eq!(q.steal(), Some(3));
        assert_eq!(q.steal(), None);
    }

    #[test]
    fn push_steal_fifo_order() {
        let q = StealQueue::new();
        for i in 0..100 { q.push(i); }
        for i in 0..100 { assert_eq!(q.steal(), Some(i)); }
        assert_eq!(q.steal(), None);
    }

    #[test]
    fn reclaim_prevents_steal() {
        let q = StealQueue::new();
        let pos = q.push(42);
        assert!(q.try_reclaim(pos)); // publisher wins
        assert_eq!(q.steal(), None); // worker finds RECLAIMED, skips, empty
    }

    #[test]
    fn steal_prevents_reclaim() {
        let q = StealQueue::new();
        let pos = q.push(42);
        assert_eq!(q.steal(), Some(42)); // worker wins
        assert!(!q.try_reclaim(pos));    // publisher loses
    }

    #[test]
    fn reclaim_vs_steal_race() {
        for _ in 0..200 {
            let q = Arc::new(StealQueue::new());
            let pos = q.push(99);

            let q2 = q.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();

            let stealer = std::thread::spawn(move || {
                b2.wait();
                q2.steal()
            });

            barrier.wait();
            let reclaimed = q.try_reclaim(pos);
            let stolen = stealer.join().unwrap();

            match (reclaimed, stolen) {
                (true, None) => {},     // publisher won
                (false, Some(99)) => {},// worker won
                other => panic!("invalid: reclaimed={}, stolen={:?}", other.0, other.1),
            }
        }
    }

    #[test]
    fn multi_producer_no_loss() {
        let q = Arc::new(StealQueue::new());
        let n_prod = 4usize;
        let per_prod = 500usize;
        let total = n_prod * per_prod;

        let handles: Vec<_> = (0..n_prod).map(|t| {
            let q = q.clone();
            std::thread::spawn(move || {
                for i in 0..per_prod { q.push((t * per_prod + i) as i64); }
            })
        }).collect();
        for h in handles { h.join().unwrap(); }

        assert_eq!(q.len(), total);
        let mut items = Vec::new();
        while let Some(v) = q.steal() { items.push(v); }
        items.sort();
        items.dedup();
        assert_eq!(items.len(), total);
    }

    #[test]
    fn concurrent_push_steal() {
        let q = Arc::new(StealQueue::new());
        let q_push = q.clone();
        let n = 10_000usize;

        let producer = std::thread::spawn(move || {
            for i in 0..n { q_push.push(i); }
        });

        let mut stolen = Vec::new();
        while stolen.len() < n {
            if let Some(v) = q.steal() { stolen.push(v); }
        }
        producer.join().unwrap();

        for i in 0..n {
            assert_eq!(stolen[i], i);
        }
    }

    #[test]
    fn multi_producer_multi_stealer() {
        let n_prod = 4u32;
        let per_prod = 500usize;
        let total = (n_prod as usize) * per_prod;

        let q = Arc::new(StealQueue::new());
        let stolen_count = Arc::new(AtomicUsize::new(0));
        let producers_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let barrier = Arc::new(Barrier::new((n_prod + 2) as usize));

        let mut producers = Vec::new();
        let mut stealers = Vec::new();

        for t in 0..n_prod {
            let q = q.clone();
            let b = barrier.clone();
            producers.push(std::thread::spawn(move || {
                b.wait();
                for i in 0..(per_prod as u32) { q.push(t * 1000 + i); }
            }));
        }

        for _ in 0..2 {
            let q = q.clone();
            let b = barrier.clone();
            let count = stolen_count.clone();
            let done = producers_done.clone();
            stealers.push(std::thread::spawn(move || {
                b.wait();
                loop {
                    if let Some(_) = q.steal() {
                        count.fetch_add(1, Ordering::AcqRel);
                    } else if done.load(Ordering::Acquire) && q.is_empty() {
                        break;
                    } else {
                        std::thread::yield_now();
                    }
                }
            }));
        }

        for h in producers { h.join().unwrap(); }
        producers_done.store(true, Ordering::Release);
        for h in stealers { h.join().unwrap(); }

        while let Some(_) = q.steal() { stolen_count.fetch_add(1, Ordering::Relaxed); }
        assert_eq!(stolen_count.load(Ordering::Relaxed), total);
    }

    #[test]
    fn segment_growth() {
        let q = StealQueue::new();
        let n = 200usize;
        for i in 0..n { q.push(i); }
        assert_eq!(q.len(), n);
        for i in 0..n { assert_eq!(q.steal(), Some(i)); }
    }

    #[test]
    fn mixed_reclaim_and_steal() {
        // Push 10 items. Reclaim evens, steal odds.
        let q = StealQueue::new();
        let positions: Vec<u64> = (0..10).map(|i| q.push(i)).collect();

        for (i, &pos) in positions.iter().enumerate() {
            if i % 2 == 0 {
                assert!(q.try_reclaim(pos), "reclaim failed for {i}");
            }
        }

        // Steal should return only odd items (evens were reclaimed)
        let mut stolen = Vec::new();
        while let Some(v) = q.steal() { stolen.push(v); }
        assert_eq!(stolen, vec![1, 3, 5, 7, 9]);
    }
}
