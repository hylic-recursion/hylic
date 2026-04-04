//! FoldChain: per-node fold accumulator with raker-based sweep.
//!
//! The raker has exclusive heap ownership. No concurrent access, no lock.
//! Events (deliver, set_total) signal via wake_pending. Only the false→true
//! transition submits a raker task. The raker clears the flag, sweeps,
//! re-checks before returning. One raker task per chain at any time.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use crate::ops::FoldOps;

pub const INITIAL_CAP: usize = 8;

// ── SlotCell ─────────────────────────────────────────

struct SlotCell<R> {
    result: UnsafeCell<MaybeUninit<R>>,
    filled: AtomicBool,
}

impl<R> SlotCell<R> {
    const fn empty() -> Self {
        SlotCell {
            result: UnsafeCell::new(MaybeUninit::uninit()),
            filled: AtomicBool::new(false),
        }
    }
}

// ── SlotBuf ──────────────────────────────────────────

struct SlotBuf<R> {
    slots: [SlotCell<R>; INITIAL_CAP],
    next: AtomicPtr<OverflowBuf<R>>,
}

impl<R> SlotBuf<R> {
    fn new() -> Self {
        SlotBuf {
            slots: std::array::from_fn(|_| SlotCell::empty()),
            next: AtomicPtr::new(std::ptr::null_mut()),
        }
    }
}

struct OverflowBuf<R> {
    slots: Box<[SlotCell<R>]>,
    next: AtomicPtr<OverflowBuf<R>>,
    capacity: usize,
}

impl<R> OverflowBuf<R> {
    fn new(capacity: usize) -> Self {
        let slots: Vec<SlotCell<R>> = (0..capacity).map(|_| SlotCell::empty()).collect();
        OverflowBuf { slots: slots.into_boxed_slice(), next: AtomicPtr::new(std::ptr::null_mut()), capacity }
    }
}

// ── SlotRef ──────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct SlotRef(u32);
unsafe impl Send for SlotRef {}
unsafe impl Sync for SlotRef {}

// ── FoldChain ────────────────────────────────────────

pub struct FoldChain<H, R> {
    heap: UnsafeCell<H>,
    first: SlotBuf<R>,
    appended: AtomicU32,
    total: AtomicU32,
    total_known: AtomicBool,
    cursor: AtomicU32,
    wake_pending: AtomicBool,
    done: AtomicBool,
    finalize_count: AtomicU32,
    id: u32, // debug: unique chain id
}

static CHAIN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

unsafe impl<H, R: Send> Send for FoldChain<H, R> {}
unsafe impl<H, R: Send> Sync for FoldChain<H, R> {}

impl<H, R> FoldChain<H, R> {
    pub fn new(heap: H) -> Self {
        let id = CHAIN_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        FoldChain {
            heap: UnsafeCell::new(heap),
            first: SlotBuf::new(),
            appended: AtomicU32::new(0),
            total: AtomicU32::new(0),
            total_known: AtomicBool::new(false),
            cursor: AtomicU32::new(0),
            wake_pending: AtomicBool::new(false),
            done: AtomicBool::new(false),
            finalize_count: AtomicU32::new(0),
            id,
        }
    }

    pub fn append_slot(&self) -> SlotRef {
        let index = self.appended.fetch_add(1, Ordering::Release);
        if (index as usize) >= INITIAL_CAP {
            self.ensure_overflow(index as usize);
        }
        SlotRef(index)
    }

    /// Store result in slot. Signal raker via wake_pending.
    /// Returns true if this call must submit the raker task (false→true transition).
    pub fn deliver(&self, slot: SlotRef, result: R) -> bool {
        assert!(!self.done.load(Ordering::Relaxed),
            "deliver to finalized chain {}: slot={}, cursor={}, total={}",
            self.id, slot.0, self.cursor.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed));
        let cell = self.slot_at(slot.0);
        unsafe { (*cell.result.get()).write(result); }
        cell.filled.store(true, Ordering::Release);
        let was_false = self.wake_pending.swap(true, Ordering::Release) == false;
        eprintln!("[chain {}] deliver slot={} submit_raker={} thread={:?}",
            self.id, slot.0, was_false, std::thread::current().id());
        was_false
    }

    /// Mark total known. Signal raker via wake_pending.
    /// Returns true if this call must submit the raker task (false→true transition).
    pub fn set_total(&self) -> bool {
        let total = self.appended.load(Ordering::Acquire);
        self.total.store(total, Ordering::Release);
        self.total_known.store(true, Ordering::Release);
        let was_false = self.wake_pending.swap(true, Ordering::Release) == false;
        eprintln!("[chain {}] set_total={} submit_raker={} thread={:?}",
            self.id, total, was_false, std::thread::current().id());
        was_false
    }

    /// The raker. Exclusive heap access — one raker task per chain at a time.
    /// Clears wake_pending, sweeps, re-checks. Returns Some(R) if finalized.
    pub fn rake<N>(&self, fold: &impl FoldOps<N, H, R>) -> Option<R> {
        eprintln!("[chain {}] rake ENTER cursor={} appended={} total_known={} total={} done={} thread={:?}",
            self.id, self.cursor.load(Ordering::Relaxed), self.appended.load(Ordering::Relaxed),
            self.total_known.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed),
            self.done.load(Ordering::Relaxed), std::thread::current().id());
        let heap = unsafe { &mut *self.heap.get() };
        let mut pos = self.cursor.load(Ordering::Relaxed);

        loop {
            self.wake_pending.store(false, Ordering::SeqCst);

            // Sweep contiguous filled slots
            loop {
                let appended = self.appended.load(Ordering::Acquire);
                if pos >= appended { break; }
                let cell = self.slot_at(pos);
                if !cell.filled.load(Ordering::Acquire) { break; }
                fold.accumulate(heap, unsafe { (*cell.result.get()).assume_init_ref() });
                pos += 1;
            }
            self.cursor.store(pos, Ordering::Release);

            // Check finalization
            if self.total_known.load(Ordering::Acquire) && pos >= self.total.load(Ordering::Acquire) {
                let prev = self.finalize_count.fetch_add(1, Ordering::Relaxed);
                assert!(prev == 0,
                    "FoldChain finalized {} times! cursor={}, total={}, thread={:?}",
                    prev + 1, pos, self.total.load(Ordering::Relaxed), std::thread::current().id());
                self.done.store(true, Ordering::Release);
                return Some(fold.finalize(unsafe { &*self.heap.get() }));
            }

            // Re-check: did events arrive during our sweep?
            if !self.wake_pending.load(Ordering::Acquire) {
                return None; // No pending events. Raker goes idle.
            }
            // Events arrived — loop and sweep again.
        }
    }

    fn slot_at(&self, index: u32) -> &SlotCell<R> {
        let idx = index as usize;
        if idx < INITIAL_CAP {
            return &self.first.slots[idx];
        }
        let mut remaining = idx - INITIAL_CAP;
        let mut ptr = self.first.next.load(Ordering::Acquire);
        loop {
            assert!(!ptr.is_null(), "slot_at: index {} beyond allocated buffers", index);
            let buf = unsafe { &*ptr };
            if remaining < buf.capacity { return &buf.slots[remaining]; }
            remaining -= buf.capacity;
            ptr = buf.next.load(Ordering::Acquire);
        }
    }

    fn ensure_overflow(&self, idx: usize) {
        let mut covered = INITIAL_CAP;
        let mut tail_next = &self.first.next;
        loop {
            let ptr = tail_next.load(Ordering::Acquire);
            if ptr.is_null() {
                let new_cap = covered;
                let new_buf = Box::into_raw(Box::new(OverflowBuf::new(new_cap)));
                match tail_next.compare_exchange(
                    std::ptr::null_mut(), new_buf, Ordering::AcqRel, Ordering::Acquire,
                ) {
                    Ok(_) => {
                        covered += new_cap;
                        if idx < covered { return; }
                        tail_next = unsafe { &(*new_buf).next };
                    }
                    Err(existing) => {
                        unsafe { drop(Box::from_raw(new_buf)); }
                        let buf = unsafe { &*existing };
                        covered += buf.capacity;
                        if idx < covered { return; }
                        tail_next = &buf.next;
                    }
                }
            } else {
                let buf = unsafe { &*ptr };
                covered += buf.capacity;
                if idx < covered { return; }
                tail_next = &buf.next;
            }
        }
    }
}

impl<H, R> Drop for FoldChain<H, R> {
    fn drop(&mut self) {
        let mut ptr = *self.first.next.get_mut();
        while !ptr.is_null() {
            let buf = unsafe { Box::from_raw(ptr) };
            ptr = buf.next.load(Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SumFold;
    impl FoldOps<(), i32, i32> for SumFold {
        fn init(&self, _: &()) -> i32 { 0 }
        fn accumulate(&self, h: &mut i32, r: &i32) { *h += r; }
        fn finalize(&self, h: &i32) -> i32 { *h }
    }

    #[test]
    fn single_child_deliver_then_total() {
        let chain = FoldChain::new(0i32);
        let s = chain.append_slot();
        assert!(chain.deliver(s, 42));  // first event → must submit raker
        assert_eq!(chain.rake::<()>(&SumFold), None); // total not known
        assert!(chain.set_total());     // wake_pending was cleared by rake
        assert_eq!(chain.rake::<()>(&SumFold), Some(42));
    }

    #[test]
    fn single_child_total_then_deliver() {
        let chain = FoldChain::new(0i32);
        let s = chain.append_slot();
        assert!(chain.set_total());
        assert_eq!(chain.rake::<()>(&SumFold), None); // slot not filled
        assert!(chain.deliver(s, 42));  // wake_pending was cleared by rake
        assert_eq!(chain.rake::<()>(&SumFold), Some(42));
    }

    #[test]
    fn three_in_order() {
        let chain = FoldChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        let s2 = chain.append_slot();
        chain.set_total();
        chain.deliver(s0, 10);
        chain.deliver(s1, 20);
        chain.deliver(s2, 30);
        // Multiple events coalesced — only one rake needed
        assert_eq!(chain.rake::<()>(&SumFold), Some(60));
    }

    #[test]
    fn three_reverse() {
        let chain = FoldChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        let s2 = chain.append_slot();
        chain.set_total();
        chain.deliver(s2, 30);
        chain.deliver(s1, 20);
        chain.deliver(s0, 10);
        assert_eq!(chain.rake::<()>(&SumFold), Some(60));
    }

    #[test]
    fn all_delivered_before_total() {
        let chain = FoldChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        chain.deliver(s0, 10);
        chain.deliver(s1, 20);
        assert_eq!(chain.rake::<()>(&SumFold), None); // total unknown
        chain.set_total();
        assert_eq!(chain.rake::<()>(&SumFold), Some(30));
    }

    #[test]
    fn overflow_beyond_initial_cap() {
        let chain = FoldChain::new(0i32);
        let n = 20u32;
        let slots: Vec<SlotRef> = (0..n).map(|_| chain.append_slot()).collect();
        chain.set_total();
        for (i, s) in slots.iter().enumerate() {
            chain.deliver(*s, (i + 1) as i32);
        }
        assert_eq!(chain.rake::<()>(&SumFold), Some(210));
    }

    #[test]
    fn concurrent_delivery() {
        use std::sync::{Arc, Barrier};
        for _ in 0..200 {
            let chain = Arc::new(FoldChain::new(0i32));
            let s0 = chain.append_slot();
            let s1 = chain.append_slot();
            chain.set_total();

            let c2 = chain.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();

            let t = std::thread::spawn(move || {
                b2.wait();
                c2.deliver(s1, 20);
                c2.rake::<()>(&SumFold)
            });
            barrier.wait();
            chain.deliver(s0, 10);
            let r1 = chain.rake::<()>(&SumFold);
            let r2 = t.join().unwrap();

            // Exactly one rake finalizes.
            let result = r1.or(r2).or_else(|| chain.rake::<()>(&SumFold));
            assert_eq!(result, Some(30));
        }
    }
}
