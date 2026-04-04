//! FoldChain: per-node fold accumulator with inline-first raker.
//!
//! Slots: segmented buffer (inline [SlotCell; 8] + linked overflow).
//! Deliver and set_total are pure stores.
//! rake() CAS's the sweeping flag for exclusive heap access, sweeps
//! contiguous filled slots, checks finalization. Done check inside
//! the gate prevents double finalization.

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
    sweeping: AtomicBool,
    done: AtomicBool,
    finalize_count: AtomicU32,
}

unsafe impl<H, R: Send> Send for FoldChain<H, R> {}
unsafe impl<H, R: Send> Sync for FoldChain<H, R> {}

impl<H, R> FoldChain<H, R> {
    pub fn new(heap: H) -> Self {
        FoldChain {
            heap: UnsafeCell::new(heap),
            first: SlotBuf::new(),
            appended: AtomicU32::new(0),
            total: AtomicU32::new(0),
            total_known: AtomicBool::new(false),
            cursor: AtomicU32::new(0),
            sweeping: AtomicBool::new(false),
            done: AtomicBool::new(false),
            finalize_count: AtomicU32::new(0),
        }
    }

    pub fn append_slot(&self) -> SlotRef {
        let index = self.appended.fetch_add(1, Ordering::Release);
        if (index as usize) >= INITIAL_CAP {
            self.ensure_overflow(index as usize);
        }
        SlotRef(index)
    }

    /// Pure store: write result to slot, set filled.
    pub fn deliver(&self, slot: SlotRef, result: R) {
        assert!(!self.done.load(Ordering::Relaxed),
            "deliver to finalized chain: slot={}", slot.0);
        let cell = self.slot_at(slot.0);
        unsafe { (*cell.result.get()).write(result); }
        cell.filled.store(true, Ordering::Release);
    }

    /// Pure store: mark total known.
    pub fn set_total(&self) {
        let total = self.appended.load(Ordering::Acquire);
        self.total.store(total, Ordering::Release);
        self.total_known.store(true, Ordering::Release);
    }

    /// The raker. CAS sweeping for exclusive heap access. Sweeps contiguous
    /// filled slots, accumulates in order, checks finalization.
    /// Returns Some(R) if this call finalized the chain.
    /// Returns None if CAS failed (another sweeper) or chain not yet complete.
    pub fn rake<N>(&self, fold: &impl FoldOps<N, H, R>) -> Option<R> {
        if self.sweeping.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            return None;
        }

        // Inside the gate. The Acquire on the CAS synchronizes with
        // the previous sweeper's Release on sweeping.store(false).
        // All their stores (done, cursor, heap mutations) are visible.
        if self.done.load(Ordering::Relaxed) {
            self.sweeping.store(false, Ordering::Release);
            return None;
        }

        let heap = unsafe { &mut *self.heap.get() };
        let mut pos = self.cursor.load(Ordering::Relaxed);

        loop {
            let appended = self.appended.load(Ordering::Acquire);
            if pos >= appended { break; }
            let cell = self.slot_at(pos);
            if !cell.filled.load(Ordering::Acquire) { break; }
            fold.accumulate(heap, unsafe { (*cell.result.get()).assume_init_ref() });
            pos += 1;
        }

        self.cursor.store(pos, Ordering::Release);

        if self.total_known.load(Ordering::Acquire) && pos >= self.total.load(Ordering::Acquire) {
            let prev = self.finalize_count.fetch_add(1, Ordering::Relaxed);
            assert!(prev == 0,
                "FoldChain double finalization: count={}, cursor={}, total={}, thread={:?}",
                prev + 1, pos, self.total.load(Ordering::Relaxed), std::thread::current().id());
            self.done.store(true, Ordering::Release);
            let result = fold.finalize(unsafe { &*self.heap.get() });
            self.sweeping.store(false, Ordering::Release);
            return Some(result);
        }

        self.sweeping.store(false, Ordering::Release);
        None
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
    fn deliver_then_total() {
        let c = FoldChain::new(0i32);
        let s = c.append_slot();
        c.deliver(s, 42);
        assert_eq!(c.rake::<()>(&SumFold), None); // total not known
        c.set_total();
        assert_eq!(c.rake::<()>(&SumFold), Some(42));
    }

    #[test]
    fn total_then_deliver() {
        let c = FoldChain::new(0i32);
        let s = c.append_slot();
        c.set_total();
        assert_eq!(c.rake::<()>(&SumFold), None); // slot not filled
        c.deliver(s, 42);
        assert_eq!(c.rake::<()>(&SumFold), Some(42));
    }

    #[test]
    fn three_in_order() {
        let c = FoldChain::new(0i32);
        let s0 = c.append_slot();
        let s1 = c.append_slot();
        let s2 = c.append_slot();
        c.set_total();
        c.deliver(s0, 10);
        c.deliver(s1, 20);
        c.deliver(s2, 30);
        assert_eq!(c.rake::<()>(&SumFold), Some(60));
    }

    #[test]
    fn three_reverse() {
        let c = FoldChain::new(0i32);
        let s0 = c.append_slot();
        let s1 = c.append_slot();
        let s2 = c.append_slot();
        c.set_total();
        c.deliver(s2, 30);
        c.deliver(s1, 20);
        c.deliver(s0, 10);
        assert_eq!(c.rake::<()>(&SumFold), Some(60));
    }

    #[test]
    fn all_before_total() {
        let c = FoldChain::new(0i32);
        let s0 = c.append_slot();
        let s1 = c.append_slot();
        c.deliver(s0, 10);
        c.deliver(s1, 20);
        assert_eq!(c.rake::<()>(&SumFold), None);
        c.set_total();
        assert_eq!(c.rake::<()>(&SumFold), Some(30));
    }

    #[test]
    fn overflow() {
        let c = FoldChain::new(0i32);
        let slots: Vec<SlotRef> = (0..20u32).map(|_| c.append_slot()).collect();
        c.set_total();
        for (i, s) in slots.iter().enumerate() {
            c.deliver(*s, (i + 1) as i32);
        }
        assert_eq!(c.rake::<()>(&SumFold), Some(210));
    }

    #[test]
    fn concurrent_delivery() {
        use std::sync::{Arc, Barrier};
        for _ in 0..200 {
            let c = Arc::new(FoldChain::new(0i32));
            let s0 = c.append_slot();
            let s1 = c.append_slot();
            c.set_total();

            let c2 = c.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();

            let t = std::thread::spawn(move || {
                b2.wait();
                c2.deliver(s1, 20);
                c2.rake::<()>(&SumFold)
            });
            barrier.wait();
            c.deliver(s0, 10);
            let r1 = c.rake::<()>(&SumFold);
            let r2 = t.join().unwrap();

            let result = r1.or(r2).or_else(|| c.rake::<()>(&SumFold));
            assert_eq!(result, Some(30));
        }
    }

    #[test]
    fn done_prevents_double_finalize() {
        let c = FoldChain::new(0i32);
        let s = c.append_slot();
        c.set_total();
        c.deliver(s, 42);
        assert_eq!(c.rake::<()>(&SumFold), Some(42));
        // Second rake: done check inside gate prevents double finalize
        assert_eq!(c.rake::<()>(&SumFold), None);
    }
}
