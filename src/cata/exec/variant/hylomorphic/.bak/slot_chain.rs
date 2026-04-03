//! SlotChain: self-driving ordered fold over asynchronous results.
//!
//! A linked list of slots, one per child. Each delivery checks: am I
//! the next expected position? If yes: accumulate into heap, advance
//! cursor, drain any parked successors. If no: park (mark filled).
//!
//! The cursor can only advance through contiguous filled slots. The
//! thread that fills the gap becomes the accumulator — drives the fold
//! forward as far as possible. When the cursor reaches the end: finalize.
//!
//! This is the ONLY unsafe code in the hylomorphic executor.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use crate::ops::FoldOps;

/// One result slot. Heap-allocated, linked to next sibling's slot.
pub struct Slot<R> {
    result: UnsafeCell<MaybeUninit<R>>,
    filled: AtomicBool,
    pub next: AtomicPtr<Slot<R>>,
}

/// The self-driving fold for one interior node.
pub struct SlotChain<H, R> {
    heap: UnsafeCell<H>,
    head: AtomicPtr<Slot<R>>,
    tail: AtomicPtr<Slot<R>>,
    /// Next position to accumulate. Only the thread that CAS's this
    /// from K to K+1 may call accumulate(heap, slot[K].result).
    cursor: AtomicU32,
    /// Total children. Set when cursor exhausts.
    total: AtomicU32,
    total_known: AtomicBool,
}

unsafe impl<H: Send, R: Send> Send for SlotChain<H, R> {}
unsafe impl<H: Send, R: Send> Sync for SlotChain<H, R> {}
unsafe impl<R: Send> Send for Slot<R> {}
unsafe impl<R: Send> Sync for Slot<R> {}

impl<H, R> SlotChain<H, R> {
    pub fn new(heap: H) -> Self {
        SlotChain {
            heap: UnsafeCell::new(heap),
            head: AtomicPtr::new(std::ptr::null_mut()),
            tail: AtomicPtr::new(std::ptr::null_mut()),
            cursor: AtomicU32::new(0),
            total: AtomicU32::new(0),
            total_known: AtomicBool::new(false),
        }
    }

    /// Append a slot. Returns a raw pointer (stable — heap allocated).
    /// Called sequentially by the cursor-pulling thread.
    pub fn append_slot(&self) -> *const Slot<R> {
        let slot = Box::into_raw(Box::new(Slot {
            result: UnsafeCell::new(MaybeUninit::uninit()),
            filled: AtomicBool::new(false),
            next: AtomicPtr::new(std::ptr::null_mut()),
        }));
        let old_tail = self.tail.swap(slot, Ordering::AcqRel);
        if old_tail.is_null() {
            self.head.store(slot, Ordering::Release);
        } else {
            unsafe { (*old_tail).next.store(slot, Ordering::Release); }
        }
        slot
    }

    /// Mark total children known. If the cursor has already reached
    /// this total, returns true (caller must finalize).
    pub fn set_total(&self, total: u32) -> bool {
        self.total.store(total, Ordering::Release);
        self.total_known.store(true, Ordering::Release);
        self.cursor.load(Ordering::Acquire) >= total
    }

    /// Deliver a result. Drives the fold forward if this result fills
    /// the gap at the cursor. Returns Some(final_result) if this
    /// delivery completed the fold (cursor reached total).
    pub fn deliver<N>(
        &self,
        slot: *const Slot<R>,
        result: R,
        fold: &impl FoldOps<N, H, R>,
    ) -> Option<R> {
        // Write result to slot, mark filled.
        unsafe {
            (*(*slot).result.get()).write(result);
            (*slot).filled.store(true, Ordering::Release);
        }

        // Try to drive the cursor forward.
        self.try_advance(fold)
    }

    /// Attempt to advance the cursor through contiguous filled slots.
    /// The thread that CAS's cursor from K to K+1 accumulates slot K.
    /// Returns Some(final_result) if the cursor reaches total.
    pub fn try_advance<N>(&self, fold: &impl FoldOps<N, H, R>) -> Option<R> {
        loop {
            let pos = self.cursor.load(Ordering::Acquire);

            // Check if we've reached the end
            if self.total_known.load(Ordering::Acquire)
                && pos >= self.total.load(Ordering::Acquire)
            {
                // All accumulated. Finalize.
                let heap = unsafe { &mut *self.heap.get() };
                return Some(fold.finalize(heap));
            }

            // Find the slot at position `pos` by walking the chain.
            let slot = self.slot_at(pos);
            if slot.is_null() {
                return None; // slot not yet appended (cursor-puller is slow)
            }
            let slot_ref = unsafe { &*slot };

            // Is this slot filled?
            if !slot_ref.filled.load(Ordering::Acquire) {
                return None; // gap — can't advance further
            }

            // Try to claim this position (CAS cursor: pos → pos+1).
            // Only one thread succeeds — that thread accumulates.
            if self.cursor.compare_exchange(
                pos, pos + 1, Ordering::AcqRel, Ordering::Relaxed
            ).is_err() {
                // Another thread advanced the cursor. Retry.
                continue;
            }

            // We own position `pos`. Accumulate into heap.
            let heap = unsafe { &mut *self.heap.get() };
            fold.accumulate(heap, unsafe { (*slot_ref.result.get()).assume_init_ref() });

            // Loop: try to advance further (drain contiguous filled slots).
        }
    }

    /// Walk the chain to find slot at position `pos`. O(pos) worst case,
    /// but in practice the cursor walks forward one step at a time.
    fn slot_at(&self, pos: u32) -> *const Slot<R> {
        let mut current = self.head.load(Ordering::Acquire);
        for _ in 0..pos {
            if current.is_null() { return std::ptr::null(); }
            current = unsafe { (*current).next.load(Ordering::Acquire) };
        }
        current
    }
}

impl<H, R> Drop for SlotChain<H, R> {
    fn drop(&mut self) {
        let mut current = *self.head.get_mut();
        while !current.is_null() {
            let slot = unsafe { Box::from_raw(current) };
            // If filled and not yet accumulated (cursor didn't reach it),
            // drop the result.
            if *slot.filled.get_mut() {
                unsafe { (*slot.result.get()).assume_init_drop(); }
            }
            current = *slot.next.get_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SumFold;
    impl FoldOps<(), i32, i32> for SumFold {
        fn init(&self, _: &()) -> i32 { 0 }
        fn accumulate(&self, heap: &mut i32, result: &i32) { *heap += result; }
        fn finalize(&self, heap: &i32) -> i32 { *heap }
    }

    #[test]
    fn single_child() {
        let chain = SlotChain::new(0i32);
        let s = chain.append_slot();
        chain.set_total(1);
        assert_eq!(chain.deliver::<()>(s, 42, &SumFold), Some(42));
    }

    #[test]
    fn in_order() {
        let chain = SlotChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        let s2 = chain.append_slot();
        chain.set_total(3);

        // Each delivery drives the cursor forward immediately
        assert_eq!(chain.deliver::<()>(s0, 10, &SumFold), None); // accumulated, cursor=1
        assert_eq!(chain.deliver::<()>(s1, 20, &SumFold), None); // accumulated, cursor=2
        assert_eq!(chain.deliver::<()>(s2, 30, &SumFold), Some(60)); // accumulated, cursor=3, finalize
    }

    #[test]
    fn out_of_order() {
        let chain = SlotChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        let s2 = chain.append_slot();
        chain.set_total(3);

        // Deliver out of order — cursor waits for the gap
        assert_eq!(chain.deliver::<()>(s2, 30, &SumFold), None); // parked (cursor=0, gap at 0)
        assert_eq!(chain.deliver::<()>(s0, 10, &SumFold), None); // accumulated 0, then 1? no, 1 not filled
                                                                   // cursor=1, gap at 1
        assert_eq!(chain.deliver::<()>(s1, 20, &SumFold), Some(60)); // fills gap, drains 1+2, finalize
    }

    #[test]
    fn total_set_after_all_delivered() {
        let chain = SlotChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();

        // Deliver before total known
        assert_eq!(chain.deliver::<()>(s0, 10, &SumFold), None);
        assert_eq!(chain.deliver::<()>(s1, 20, &SumFold), None);

        // set_total: cursor is already at 2, returns true
        assert!(chain.set_total(2));
        // Caller must call try_advance to get the final result
        assert_eq!(chain.try_advance::<()>(&SumFold), Some(30));
    }

    #[test]
    fn concurrent_delivery() {
        use std::sync::{Arc, Barrier};

        for _ in 0..100 {
            let chain = Arc::new(SlotChain::new(0i32));
            let s0 = chain.append_slot();
            let s1 = chain.append_slot();
            chain.set_total(2);

            let c2 = chain.clone();
            let barrier = Arc::new(Barrier::new(2));
            let b2 = barrier.clone();

            let t = std::thread::spawn(move || {
                b2.wait();
                c2.deliver::<()>(s1, 20, &SumFold)
            });

            barrier.wait();
            let r0 = chain.deliver::<()>(s0, 10, &SumFold);
            let r1 = t.join().unwrap();

            match (r0, r1) {
                (Some(30), None) | (None, Some(30)) => {},
                other => panic!("unexpected: {:?}", other),
            }
        }
    }
}
