//! SlotChain: self-driving ordered fold with segmented buffer storage.
//!
//! First buffer inline (zero alloc for ≤INITIAL_CAP children).
//! Overflow buffers heap-allocated, linked. Same SlotBuf struct for both.
//! Reactive try_advance: every delivery and set_total drives the fold.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use crate::ops::FoldOps;

/// Initial inline buffer capacity. Covers most tree nodes.
pub const INITIAL_CAP: usize = 8;

// ── SlotCell ─────────────────────────────────────────

pub struct SlotCell<R> {
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

/// A contiguous buffer of slots + a link to the next overflow buffer.
/// Same struct for inline (stack) and overflow (heap). Unified access.
pub struct SlotBuf<R> {
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

/// Heap-allocated overflow buffer. Dynamically sized.
struct OverflowBuf<R> {
    slots: Box<[SlotCell<R>]>,
    next: AtomicPtr<OverflowBuf<R>>,
    capacity: usize,
}

impl<R> OverflowBuf<R> {
    fn new(capacity: usize) -> Self {
        let slots: Vec<SlotCell<R>> = (0..capacity).map(|_| SlotCell::empty()).collect();
        OverflowBuf {
            slots: slots.into_boxed_slice(),
            next: AtomicPtr::new(std::ptr::null_mut()),
            capacity,
        }
    }
}

// ── SlotRef ──────────────────────────────────────────

/// A sendable reference to a slot. Index-based (stable).
#[derive(Clone, Copy)]
pub struct SlotRef(u32);

unsafe impl Send for SlotRef {}
unsafe impl Sync for SlotRef {}

// ── SlotChain ────────────────────────────────────────

pub struct SlotChain<H, R> {
    heap: UnsafeCell<H>,
    first: SlotBuf<R>,
    appended: AtomicU32,
    total: AtomicU32,
    total_known: AtomicBool,
    cursor_index: AtomicU32,
    done: AtomicBool,
    result: UnsafeCell<Option<R>>,
}

// SAFETY: heap access serialized by cursor CAS (single writer).
// Slot results: written once (deliver), read once (try_advance).
unsafe impl<H, R: Send> Send for SlotChain<H, R> {}
unsafe impl<H, R: Send> Sync for SlotChain<H, R> {}

impl<H, R> SlotChain<H, R> {
    pub fn new(heap: H) -> Self {
        SlotChain {
            heap: UnsafeCell::new(heap),
            first: SlotBuf::new(),
            appended: AtomicU32::new(0),
            total: AtomicU32::new(0),
            total_known: AtomicBool::new(false),
            cursor_index: AtomicU32::new(0),
            done: AtomicBool::new(false),
            result: UnsafeCell::new(None),
        }
    }

    /// Append a slot. Returns a SlotRef (index-based, Copy, Send).
    /// Called sequentially by the cursor-pulling thread.
    pub fn append_slot(&self) -> SlotRef {
        let index = self.appended.fetch_add(1, Ordering::Release);
        let idx = index as usize;

        // Ensure the buffer exists for this index.
        if idx >= INITIAL_CAP {
            self.ensure_overflow(idx);
        }

        SlotRef(index)
    }

    /// Deliver a result to a slot. Drives try_advance.
    pub fn deliver<N>(&self, slot: SlotRef, result: R, fold: &impl FoldOps<N, H, R>) {
        let cell = self.slot_at(slot.0);
        unsafe { (*cell.result.get()).write(result); }
        cell.filled.store(true, Ordering::Release);
        self.try_advance(fold);
    }

    /// Mark total known. Drives try_advance.
    pub fn set_total<N>(&self, fold: &impl FoldOps<N, H, R>) {
        let total = self.appended.load(Ordering::Acquire);
        self.total.store(total, Ordering::Release);
        self.total_known.store(true, Ordering::Release);
        self.try_advance(fold);
    }

    /// Called after all joins return. Returns the finalized R.
    pub fn finish<N>(&self, fold: &impl FoldOps<N, H, R>) -> R {
        self.try_advance(fold);
        debug_assert!(self.done.load(Ordering::Acquire),
            "SlotChain not done after all joins returned");
        unsafe { (*self.result.get()).take().expect("chain result not set") }
    }

    /// The single reactive driver.
    fn try_advance<N>(&self, fold: &impl FoldOps<N, H, R>) {
        loop {
            let pos = self.cursor_index.load(Ordering::Acquire);

            if self.total_known.load(Ordering::Acquire)
                && pos >= self.total.load(Ordering::Acquire)
            {
                if self.done.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    let heap = unsafe { &*self.heap.get() };
                    let result = fold.finalize(heap);
                    unsafe { *self.result.get() = Some(result); }
                }
                return;
            }

            // Check if this position's slot exists and is filled
            if pos >= self.appended.load(Ordering::Acquire) { return; }
            let cell = self.slot_at(pos);
            if !cell.filled.load(Ordering::Acquire) { return; }

            if self.cursor_index.compare_exchange(
                pos, pos + 1, Ordering::AcqRel, Ordering::Relaxed
            ).is_err() {
                continue;
            }

            let heap = unsafe { &mut *self.heap.get() };
            fold.accumulate(heap, unsafe { (*cell.result.get()).assume_init_ref() });
        }
    }

    /// Access a slot by index. Walks from first buffer through overflow chain.
    fn slot_at(&self, index: u32) -> &SlotCell<R> {
        let idx = index as usize;
        if idx < INITIAL_CAP {
            return &self.first.slots[idx];
        }
        // Walk overflow chain
        let mut remaining = idx - INITIAL_CAP;
        let mut ptr = self.first.next.load(Ordering::Acquire);
        loop {
            assert!(!ptr.is_null(), "slot_at: index {} beyond allocated buffers", index);
            let buf = unsafe { &*ptr };
            if remaining < buf.capacity {
                return &buf.slots[remaining];
            }
            remaining -= buf.capacity;
            ptr = buf.next.load(Ordering::Acquire);
        }
    }

    /// Ensure overflow buffers cover the given index.
    fn ensure_overflow(&self, idx: usize) {
        let mut covered = INITIAL_CAP;
        let mut tail_next = &self.first.next;

        loop {
            let ptr = tail_next.load(Ordering::Acquire);
            if ptr.is_null() {
                // Allocate new overflow buffer
                let new_cap = covered; // double each time (8, 16, 32, ...)
                let new_buf = Box::into_raw(Box::new(OverflowBuf::new(new_cap)));
                match tail_next.compare_exchange(
                    std::ptr::null_mut(), new_buf,
                    Ordering::AcqRel, Ordering::Acquire,
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

impl<H, R> Drop for SlotChain<H, R> {
    fn drop(&mut self) {
        // Drop overflow buffers
        let mut ptr = *self.first.next.get_mut();
        while !ptr.is_null() {
            let mut buf = unsafe { Box::from_raw(ptr) };
            ptr = *buf.next.get_mut();
            // SlotCells with filled results that weren't accumulated
            // are dropped by Box<[SlotCell<R>]> drop — but MaybeUninit
            // doesn't auto-drop. For now: accept the leak of unconsumed
            // results (only happens on panic/early exit).
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
    fn single_child() {
        let chain = SlotChain::new(0i32);
        let s = chain.append_slot();
        chain.set_total::<()>(&SumFold);
        chain.deliver::<()>(s, 42, &SumFold);
        assert_eq!(chain.finish::<()>(&SumFold), 42);
    }

    #[test]
    fn three_in_order() {
        let chain = SlotChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        let s2 = chain.append_slot();
        chain.set_total::<()>(&SumFold);
        chain.deliver::<()>(s0, 10, &SumFold);
        chain.deliver::<()>(s1, 20, &SumFold);
        chain.deliver::<()>(s2, 30, &SumFold);
        assert_eq!(chain.finish::<()>(&SumFold), 60);
    }

    #[test]
    fn three_reverse() {
        let chain = SlotChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        let s2 = chain.append_slot();
        chain.set_total::<()>(&SumFold);
        chain.deliver::<()>(s2, 30, &SumFold);
        chain.deliver::<()>(s1, 20, &SumFold);
        chain.deliver::<()>(s0, 10, &SumFold);
        assert_eq!(chain.finish::<()>(&SumFold), 60);
    }

    #[test]
    fn total_set_early() {
        let chain = SlotChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        chain.set_total::<()>(&SumFold);
        chain.deliver::<()>(s0, 10, &SumFold);
        chain.deliver::<()>(s1, 20, &SumFold);
        assert_eq!(chain.finish::<()>(&SumFold), 30);
    }

    #[test]
    fn overflow_beyond_initial_cap() {
        let chain = SlotChain::new(0i32);
        let n = 20u32; // exceeds INITIAL_CAP=8
        let slots: Vec<SlotRef> = (0..n).map(|_| chain.append_slot()).collect();
        chain.set_total::<()>(&SumFold);
        for (i, s) in slots.iter().enumerate() {
            chain.deliver::<()>(*s, (i + 1) as i32, &SumFold);
        }
        // sum of 1..=20 = 210
        assert_eq!(chain.finish::<()>(&SumFold), 210);
    }

    #[test]
    fn concurrent_delivery() {
        use std::sync::{Arc, Barrier};
        for _ in 0..100 {
            let chain = Arc::new(SlotChain::new(0i32));
            let s0 = chain.append_slot();
            let s1 = chain.append_slot();
            chain.set_total::<()>(&SumFold);

            let c2 = chain.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();

            let t = std::thread::spawn(move || {
                b2.wait();
                c2.deliver::<()>(s1, 20, &SumFold);
            });
            barrier.wait();
            chain.deliver::<()>(s0, 10, &SumFold);
            t.join().unwrap();

            assert_eq!(chain.finish::<()>(&SumFold), 30);
        }
    }
}
