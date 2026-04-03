//! SharedDeque<T>: lock-free unbounded MPMC work-stealing deque.
//!
//! Any thread can push, pop (LIFO), or steal (FIFO). Send + Sync.
//! No thread affinity, no thread_local, no globals.
//!
//! Protocol:
//! - Push: claim slot via per-slot CAS, write value, publish via fetch_add on bottom.
//! - Pop: CAS on bottom (decrement). LIFO — most recently pushed.
//! - Steal: CAS on top (increment). FIFO — oldest.
//! - Resize: stop-the-world with quiescent barrier. Resize waits for all
//!   in-flight pushes to drain before copying and swapping the buffer.
//!
//! Auto-resizing. Old buffers kept alive until drop (no epoch GC).

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering::*};
use std::sync::Mutex;

const INITIAL_CAPACITY: usize = 64;

// ── Slot ─────────────────────────────────────────────

struct Slot<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    /// true = slot has been claimed by a pusher and value is (or will be) written.
    /// Cleared after a pop/steal consumes the value.
    claimed: AtomicBool,
}

impl<T> Slot<T> {
    fn new_empty() -> Self {
        Slot {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            claimed: AtomicBool::new(false),
        }
    }
}

// ── Buffer ───────────────────────────────────────────

struct Buffer<T> {
    slots: Box<[Slot<T>]>,
    mask: usize,
}

impl<T> Buffer<T> {
    fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        let slots: Vec<Slot<T>> = (0..capacity).map(|_| Slot::new_empty()).collect();
        Buffer { slots: slots.into_boxed_slice(), mask: capacity - 1 }
    }

    fn capacity(&self) -> usize { self.mask + 1 }

    fn slot(&self, pos: u64) -> &Slot<T> {
        let idx = (pos as usize) & self.mask;
        assert!(idx < self.slots.len(), "slot index {idx} out of bounds (cap={}, pos={pos}, mask=0x{:x})", self.slots.len(), self.mask);
        &self.slots[idx]
    }
}

// ── SharedDeque ──────────────────────────────────────

pub struct SharedDeque<T> {
    buffer: AtomicPtr<Buffer<T>>,
    bottom: CachePad<AtomicU64>,
    top: CachePad<AtomicU64>,
    /// True = resize in progress. Push/pop/steal spin on this.
    resizing: AtomicBool,
    /// Count of pushes currently between slot-claim and fetch_add.
    /// Resize waits for this to reach 0 before copying.
    in_flight: AtomicUsize,
    /// Old buffers kept alive until deque drops.
    retired: Mutex<Vec<Box<Buffer<T>>>>,
}

unsafe impl<T: Send> Send for SharedDeque<T> {}
unsafe impl<T: Send> Sync for SharedDeque<T> {}

impl<T> SharedDeque<T> {
    pub fn new() -> Self {
        let buf = Box::new(Buffer::new(INITIAL_CAPACITY));
        SharedDeque {
            buffer: AtomicPtr::new(Box::into_raw(buf)),
            bottom: CachePad(AtomicU64::new(0)),
            top: CachePad(AtomicU64::new(0)),
            resizing: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            retired: Mutex::new(Vec::new()),
        }
    }

    fn buf(&self) -> &Buffer<T> {
        unsafe { &*self.buffer.load(Acquire) }
    }

    // ── Push ─────────────────────────────────────────
    //
    // 1. Spin if resizing.
    // 2. Increment in_flight (announces we're about to operate on the buffer).
    // 3. Re-check resizing — if it started between step 1 and 2, back off.
    // 4. Load buf, b, t. Check capacity. Claim slot. Write. Publish.
    // 5. Decrement in_flight.
    //
    // The in_flight counter is the quiescent barrier: resize sets
    // resizing=true, then waits for in_flight==0. This guarantees no
    // pusher is mid-write on the old buffer when resize copies.

    pub fn push(&self, item: T) {
        loop {
            // Spin if resize in progress
            if self.resizing.load(Acquire) {
                std::hint::spin_loop();
                continue;
            }

            // Announce: we're entering a push operation on the current buffer.
            self.in_flight.fetch_add(1, AcqRel);

            // Double-check: if resize started between our resizing check and
            // in_flight increment, we must back off — the resizer might be
            // waiting for us and we'd operate on a stale buffer.
            if self.resizing.load(Acquire) {
                self.in_flight.fetch_sub(1, Release);
                std::hint::spin_loop();
                continue;
            }

            let buf = self.buf();
            let b = self.bottom.0.load(Acquire);
            let t = self.top.0.load(Acquire);

            if (b.wrapping_sub(t) as usize) >= buf.capacity() {
                // Need to resize. Release in_flight first, then grow.
                self.in_flight.fetch_sub(1, Release);
                self.grow(buf.capacity());
                continue;
            }

            let slot = buf.slot(b);

            if slot.claimed.compare_exchange_weak(false, true, AcqRel, Relaxed).is_err() {
                self.in_flight.fetch_sub(1, Release);
                std::hint::spin_loop();
                continue;
            }

            // Exclusive access to slot. Write the value.
            unsafe { (*slot.value.get()).write(item); }

            // Publish: CAS bottom from b to b+1. If a concurrent pop
            // decremented bottom, our CAS fails — release claim and retry.
            // We must "unwrite" the value to avoid leaving initialized data
            // in a slot that won't be published.
            if self.bottom.0.compare_exchange(b, b.wrapping_add(1), Release, Relaxed).is_err() {
                // Pop changed bottom. Our slot is at position b which is
                // now "behind" bottom — it was logically popped by the concurrent pop.
                // Drop the value we wrote and release the claim.
                unsafe { (*slot.value.get()).assume_init_drop(); }
                slot.claimed.store(false, Release);
                self.in_flight.fetch_sub(1, Release);
                // Re-push the item in the next iteration
                continue;
            }

            // Done — release in_flight.
            self.in_flight.fetch_sub(1, Release);
            return;
        }
    }

    // ── Pop (LIFO) ───────────────────────────────────

    pub fn pop(&self) -> Option<T> {
        loop {
            if self.resizing.load(Acquire) {
                std::hint::spin_loop();
                continue;
            }

            let b = self.bottom.0.load(Acquire);
            let t = self.top.0.load(Acquire);

            if b.wrapping_sub(t) == 0 { return None; }

            let target = b.wrapping_sub(1);

            if target != t {
                if self.bottom.0.compare_exchange_weak(b, target, AcqRel, Relaxed).is_err() {
                    continue;
                }
                let slot = self.buf().slot(target);
                let val = unsafe { (*slot.value.get()).assume_init_read() };
                slot.claimed.store(false, Release);
                return Some(val);
            }

            // Last item — CAS on top as tiebreaker.
            if self.top.0.compare_exchange(t, t.wrapping_add(1), SeqCst, Relaxed).is_ok() {
                let slot = self.buf().slot(t);
                let val = unsafe { (*slot.value.get()).assume_init_read() };
                slot.claimed.store(false, Release);
                self.bottom.0.store(t.wrapping_add(1), Release);
                return Some(val);
            } else {
                self.bottom.0.store(t.wrapping_add(1), Release);
                return None;
            }
        }
    }

    // ── Steal (FIFO) ─────────────────────────────────

    pub fn steal(&self) -> Option<T> {
        loop {
            if self.resizing.load(Acquire) {
                std::hint::spin_loop();
                continue;
            }

            let t = self.top.0.load(Acquire);
            std::sync::atomic::fence(SeqCst);
            let b = self.bottom.0.load(Acquire);

            if b.wrapping_sub(t) == 0 { return None; }

            let slot = self.buf().slot(t);

            if self.top.0.compare_exchange_weak(t, t.wrapping_add(1), SeqCst, Relaxed).is_ok() {
                let val = unsafe { (*slot.value.get()).assume_init_read() };
                slot.claimed.store(false, Release);
                return Some(val);
            }
        }
    }

    // ── Resize ───────────────────────────────────────
    //
    // 1. Claim the resizer role (CAS resizing false→true).
    // 2. Wait for in_flight == 0 (quiescent barrier — all pushers have
    //    either completed or backed off).
    // 3. Copy live elements from old buffer to new.
    // 4. Swap buffer pointer. Retire old buffer.
    // 5. Release resizing flag.

    fn grow(&self, old_cap: usize) {
        if self.resizing.compare_exchange(false, true, AcqRel, Relaxed).is_err() {
            while self.resizing.load(Acquire) { std::hint::spin_loop(); }
            return;
        }

        // Wait for all in-flight pushes to complete.
        // After this, no thread holds a reference to the old buffer.
        while self.in_flight.load(Acquire) > 0 {
            std::hint::spin_loop();
        }

        let current_buf = self.buf();
        if current_buf.capacity() > old_cap {
            // Another resize already happened.
            self.resizing.store(false, Release);
            return;
        }

        let new_cap = old_cap * 2;
        let new_buf = Buffer::new(new_cap);

        let t = self.top.0.load(Relaxed);
        let b = self.bottom.0.load(Relaxed);
        for pos in t..b {
            let old_slot = current_buf.slot(pos);
            let new_slot = new_buf.slot(pos);
            unsafe {
                let val = (*old_slot.value.get()).assume_init_read();
                (*new_slot.value.get()).write(val);
            }
            new_slot.claimed.store(true, Relaxed);
            old_slot.claimed.store(false, Relaxed);
        }

        let new_ptr = Box::into_raw(Box::new(new_buf));
        let old_ptr = self.buffer.swap(new_ptr, AcqRel);
        self.retired.lock().unwrap().push(unsafe { Box::from_raw(old_ptr) });
        self.resizing.store(false, Release);
    }

    // ── Query ────────────────────────────────────────

    pub fn is_empty(&self) -> bool {
        let t = self.top.0.load(Acquire);
        let b = self.bottom.0.load(Acquire);
        b.wrapping_sub(t) == 0
    }

    pub fn len(&self) -> usize {
        let t = self.top.0.load(Acquire);
        let b = self.bottom.0.load(Acquire);
        b.wrapping_sub(t) as usize
    }
}

impl<T> Drop for SharedDeque<T> {
    fn drop(&mut self) {
        let buf = unsafe { &*self.buffer.load(Relaxed) };
        let t = self.top.0.load(Relaxed);
        let b = self.bottom.0.load(Relaxed);
        for pos in t..b {
            let slot = buf.slot(pos);
            if slot.claimed.load(Relaxed) {
                unsafe { (*slot.value.get()).assume_init_drop(); }
            }
        }
        unsafe { drop(Box::from_raw(self.buffer.load(Relaxed))); }
    }
}

#[repr(align(128))]
struct CachePad<T>(T);

// ── Tests ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn lifo_pop() {
        let d = SharedDeque::new();
        d.push(1); d.push(2); d.push(3);
        assert_eq!(d.pop(), Some(3));
        assert_eq!(d.pop(), Some(2));
        assert_eq!(d.pop(), Some(1));
        assert_eq!(d.pop(), None);
    }

    #[test]
    fn fifo_steal() {
        let d = SharedDeque::new();
        d.push(1); d.push(2); d.push(3);
        assert_eq!(d.steal(), Some(1));
        assert_eq!(d.steal(), Some(2));
        assert_eq!(d.steal(), Some(3));
        assert_eq!(d.steal(), None);
    }

    #[test]
    fn mixed_pop_and_steal() {
        let d = SharedDeque::new();
        d.push(1); d.push(2); d.push(3); d.push(4);
        assert_eq!(d.pop(), Some(4));
        assert_eq!(d.steal(), Some(1));
        assert_eq!(d.pop(), Some(3));
        assert_eq!(d.steal(), Some(2));
        assert_eq!(d.pop(), None);
        assert_eq!(d.steal(), None);
    }

    #[test]
    fn last_item_pop_wins() {
        let d = SharedDeque::new();
        d.push(42);
        assert_eq!(d.pop(), Some(42));
        assert_eq!(d.steal(), None);
    }

    #[test]
    fn last_item_race() {
        for _ in 0..200 {
            let d = Arc::new(SharedDeque::new());
            d.push(99);
            let d2 = d.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();
            let stealer = std::thread::spawn(move || { b2.wait(); d2.steal() });
            barrier.wait();
            let popped = d.pop();
            let stolen = stealer.join().unwrap();
            match (popped, stolen) {
                (Some(99), None) | (None, Some(99)) => {},
                other => panic!("both or neither: {:?}", other),
            }
        }
    }

    #[test]
    fn push_pop_reclaim() {
        let d = SharedDeque::new();
        for _ in 0..10_000 {
            d.push(7);
            assert_eq!(d.pop(), Some(7));
        }
        assert!(d.is_empty());
    }

    #[test]
    fn concurrent_push_steal_fifo() {
        let d = Arc::new(SharedDeque::new());
        let d_push = d.clone();
        let n: usize = 10_000;
        let producer = std::thread::spawn(move || {
            for i in 0..n { d_push.push(i); }
        });
        let mut stolen = Vec::new();
        while stolen.len() < n {
            if let Some(v) = d.steal() { stolen.push(v); }
        }
        producer.join().unwrap();
        for i in 0..n {
            assert_eq!(stolen[i], i, "FIFO order broken at {}", i);
        }
    }

    #[test]
    fn multi_producer_no_loss() {
        let d = Arc::new(SharedDeque::new());
        let n_prod = 4usize;
        let per_prod = 1000usize;
        let handles: Vec<_> = (0..n_prod).map(|t| {
            let d = d.clone();
            std::thread::spawn(move || {
                for i in 0..per_prod { d.push((t * per_prod + i) as i64); }
            })
        }).collect();
        for h in handles { h.join().unwrap(); }
        let total = n_prod * per_prod;
        assert_eq!(d.len(), total);
        let mut items = Vec::new();
        while let Some(v) = d.steal() { items.push(v); }
        items.sort();
        items.dedup();
        assert_eq!(items.len(), total);
    }

    #[test]
    fn multi_producer_multi_stealer() {
        let n_prod = 4u32;
        let per_prod = 500usize;
        let total = (n_prod as usize) * per_prod; // 2000

        let d = Arc::new(SharedDeque::new());
        let stolen_count = Arc::new(AtomicUsize::new(0));
        let producers_done = Arc::new(AtomicBool::new(false));
        let barrier = Arc::new(Barrier::new((n_prod + 2) as usize));

        let mut producers = Vec::new();
        let mut stealers = Vec::new();

        for t in 0..n_prod {
            let d = d.clone();
            let b = barrier.clone();
            producers.push(std::thread::spawn(move || {
                b.wait();
                for i in 0..(per_prod as u32) { d.push(t * 1000 + i); }
            }));
        }

        for _ in 0..2 {
            let d = d.clone();
            let b = barrier.clone();
            let count = stolen_count.clone();
            let done = producers_done.clone();
            stealers.push(std::thread::spawn(move || {
                b.wait();
                loop {
                    if let Some(_) = d.steal() {
                        count.fetch_add(1, AcqRel);
                    } else {
                        let c = count.load(Acquire);
                        let dn = done.load(Acquire);
                        if dn && d.is_empty() && c >= total { break; }
                        if dn && d.is_empty() {
                            // Producers done, deque empty, but count < total.
                            // Items were lost.
                            panic!("items lost: count={c}, total={total}, deque_len={}", d.len());
                        }
                        std::thread::yield_now();
                    }
                }
            }));
        }

        for h in producers { h.join().unwrap(); }
        producers_done.store(true, Release);
        for h in stealers { h.join().unwrap(); }

        while let Some(_) = d.steal() { stolen_count.fetch_add(1, Relaxed); }
        assert_eq!(stolen_count.load(Relaxed), total);
    }

    #[test]
    fn auto_resize() {
        let d = SharedDeque::new();
        let n = 256usize;
        for i in 0..n { d.push(i); }
        assert_eq!(d.len(), n);
        for i in (0..n).rev() { assert_eq!(d.pop(), Some(i)); }
        assert!(d.is_empty());
    }

    #[test]
    fn concurrent_resize() {
        let n_threads = 4u32;
        let per_thread = 200usize;
        let total = (n_threads as usize) * per_thread;

        let d = Arc::new(SharedDeque::new());
        let barrier = Arc::new(Barrier::new(n_threads as usize));

        let handles: Vec<_> = (0..n_threads).map(|t| {
            let d = d.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                for i in 0..(per_thread as u32) { d.push(t * 1000 + i); }
            })
        }).collect();

        for h in handles { h.join().unwrap(); }
        assert_eq!(d.len(), total);

        let mut items = Vec::new();
        while let Some(v) = d.steal() { items.push(v); }
        items.sort();
        items.dedup();
        assert_eq!(items.len(), total);
    }

    #[test]
    fn push_pop_across_resize_boundary() {
        let d = SharedDeque::new();
        let n = 100usize;
        for i in 0..n { d.push(i); }
        for i in (0..n).rev() { assert_eq!(d.pop(), Some(i)); }
        for i in n..(2 * n) { d.push(i); }
        for i in (n..(2 * n)).rev() { assert_eq!(d.pop(), Some(i)); }
        assert!(d.is_empty());
    }
}
