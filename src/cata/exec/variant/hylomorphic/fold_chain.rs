//! FoldChain: per-node fold accumulator with ticket-based bulk finalization.
//!
//! Packed AtomicU64 ticket: events_done (low 32) | total (high 32).
//! Each delivery stores its result and takes a ticket. The last event
//! (by ticket) bulk-sweeps all slots and finalizes. No CAS gate, no
//! cursor, no streaming. Simple and correct baseline.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
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
pub struct SlotRef(pub(super) u32);
unsafe impl Send for SlotRef {}
unsafe impl Sync for SlotRef {}

// ── State packing ────────────────────────────────────

fn pack_total(total: u32) -> u64 { (total as u64) << 32 }
fn unpack(state: u64) -> (u32, u32) { (state as u32, (state >> 32) as u32) }

// ── FoldChain ────────────────────────────────────────

pub struct FoldChain<H, R> {
    heap: UnsafeCell<H>,
    first: SlotBuf<R>,
    appended: AtomicU32,
    state: AtomicU64,
    done: AtomicBool,
}

unsafe impl<H, R: Send> Send for FoldChain<H, R> {}
unsafe impl<H, R: Send> Sync for FoldChain<H, R> {}

impl<H, R> FoldChain<H, R> {
    pub fn new(heap: H) -> Self {
        FoldChain {
            heap: UnsafeCell::new(heap),
            first: SlotBuf::new(),
            appended: AtomicU32::new(0),
            state: AtomicU64::new(0),
            done: AtomicBool::new(false),
        }
    }

    pub fn append_slot(&self) -> SlotRef {
        let index = self.appended.fetch_add(1, Ordering::Release);
        if (index as usize) >= INITIAL_CAP {
            self.ensure_overflow(index as usize);
        }
        SlotRef(index)
    }

    /// Deliver a result. If this is the last event: bulk-sweep all slots,
    /// accumulate, finalize, return Some. Otherwise return None.
    pub fn deliver_and_finalize<N>(&self, slot: SlotRef, result: R, fold: &impl FoldOps<N, H, R>) -> Option<R> {
        let cell = self.slot_at(slot.0);
        unsafe { (*cell.result.get()).write(result); }
        cell.filled.store(true, Ordering::Release);

        let prev = self.state.fetch_add(1, Ordering::Relaxed);
        let (done_before, total) = unpack(prev);
        if total > 0 && done_before + 1 >= total {
            return Some(self.bulk_finalize(fold));
        }
        None
    }

    /// Mark total known. If all deliveries already done: bulk finalize.
    pub fn set_total_and_finalize<N>(&self, fold: &impl FoldOps<N, H, R>) -> Option<R> {
        let total = self.appended.load(Ordering::Relaxed);
        let prev = self.state.fetch_add(pack_total(total), Ordering::Relaxed);
        let (done_before, _) = unpack(prev);
        if done_before >= total {
            return Some(self.bulk_finalize(fold));
        }
        None
    }

    /// Called by exactly one event (the last, by ticket). All slots are
    /// filled. Per-slot filled.load(Acquire) provides visibility.
    fn bulk_finalize<N>(&self, fold: &impl FoldOps<N, H, R>) -> R {
        self.done.store(true, Ordering::Relaxed);
        let heap = unsafe { &mut *self.heap.get() };
        let total = self.appended.load(Ordering::Relaxed);
        for pos in 0..total {
            while !self.slot_at(pos).filled.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            fold.accumulate(heap, unsafe { (*self.slot_at(pos).result.get()).assume_init_ref() });
        }
        fold.finalize(unsafe { &*self.heap.get() })
    }

    #[cfg(test)]
    fn is_done(&self) -> bool { self.done.load(Ordering::Relaxed) }

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
    fn deliver_then_total() {
        let c = FoldChain::new(0i32);
        let s = c.append_slot();
        assert_eq!(c.deliver_and_finalize(s, 42, &SumFold), None);
        assert_eq!(c.set_total_and_finalize(&SumFold), Some(42));
    }

    #[test]
    fn total_then_deliver() {
        let c = FoldChain::new(0i32);
        let s = c.append_slot();
        assert_eq!(c.set_total_and_finalize(&SumFold), None);
        assert_eq!(c.deliver_and_finalize(s, 42, &SumFold), Some(42));
    }

    #[test]
    fn three_in_order() {
        let c = FoldChain::new(0i32);
        let s0 = c.append_slot();
        let s1 = c.append_slot();
        let s2 = c.append_slot();
        c.set_total_and_finalize(&SumFold);
        c.deliver_and_finalize(s0, 10, &SumFold);
        c.deliver_and_finalize(s1, 20, &SumFold);
        assert_eq!(c.deliver_and_finalize(s2, 30, &SumFold), Some(60));
    }

    #[test]
    fn three_reverse() {
        let c = FoldChain::new(0i32);
        let s0 = c.append_slot();
        let s1 = c.append_slot();
        let s2 = c.append_slot();
        c.set_total_and_finalize(&SumFold);
        c.deliver_and_finalize(s2, 30, &SumFold);
        c.deliver_and_finalize(s1, 20, &SumFold);
        assert_eq!(c.deliver_and_finalize(s0, 10, &SumFold), Some(60));
    }

    #[test]
    fn all_before_total() {
        let c = FoldChain::new(0i32);
        let s0 = c.append_slot();
        let s1 = c.append_slot();
        c.deliver_and_finalize(s0, 10, &SumFold);
        c.deliver_and_finalize(s1, 20, &SumFold);
        assert!(!c.is_done());
        assert_eq!(c.set_total_and_finalize(&SumFold), Some(30));
    }

    #[test]
    fn overflow() {
        let c = FoldChain::new(0i32);
        let slots: Vec<SlotRef> = (0..20u32).map(|_| c.append_slot()).collect();
        c.set_total_and_finalize(&SumFold);
        let mut last = None;
        for (i, s) in slots.iter().enumerate() {
            last = c.deliver_and_finalize(*s, (i + 1) as i32, &SumFold);
        }
        assert_eq!(last, Some(210));
    }

    #[test]
    fn concurrent_delivery() {
        use std::sync::{Arc, Barrier};
        for _ in 0..200 {
            let c = Arc::new(FoldChain::new(0i32));
            let s0 = c.append_slot();
            let s1 = c.append_slot();
            c.set_total_and_finalize(&SumFold);

            let c2 = c.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();

            let t = std::thread::spawn(move || {
                b2.wait();
                c2.deliver_and_finalize(s1, 20, &SumFold)
            });
            barrier.wait();
            let r1 = c.deliver_and_finalize(s0, 10, &SumFold);
            let r2 = t.join().unwrap();

            let result = r1.or(r2);
            assert_eq!(result, Some(30), "exactly one delivery must finalize");
        }
    }

    #[test]
    fn done_prevents_double_finalize() {
        let c = FoldChain::new(0i32);
        let s = c.append_slot();
        c.set_total_and_finalize(&SumFold);
        assert_eq!(c.deliver_and_finalize(s, 42, &SumFold), Some(42));
        assert!(c.is_done());
    }
}
