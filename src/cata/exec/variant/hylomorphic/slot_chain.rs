//! SlotChain: self-driving ordered fold over asynchronously arriving results.
//!
//! Linked list of slots. Cursor walks head-to-tail through contiguous
//! filled slots, accumulating each into the heap. One mechanism
//! (try_advance) handles all events. No polling.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use crate::ops::FoldOps;

/// Sendable wrapper for a slot pointer.
pub struct SlotPtr<R>(*const Slot<R>);
unsafe impl<R: Send> Send for SlotPtr<R> {}
unsafe impl<R: Send> Sync for SlotPtr<R> {}

pub struct Slot<R> {
    result: UnsafeCell<MaybeUninit<R>>,
    filled: AtomicBool,
    next: AtomicPtr<Slot<R>>,
}

unsafe impl<R: Send> Send for Slot<R> {}
unsafe impl<R: Send> Sync for Slot<R> {}

pub struct SlotChain<H, R> {
    heap: UnsafeCell<H>,
    result: UnsafeCell<Option<R>>,
    head: AtomicPtr<Slot<R>>,
    tail: AtomicPtr<Slot<R>>,
    /// Monotonic: how many slots have been appended.
    appended: AtomicU32,
    /// Set when cursor exhausts (total children known).
    total: AtomicU32,
    total_known: AtomicBool,
    /// Next slot to accumulate. CAS serializes access to heap.
    cursor_index: AtomicU32,
    /// Pointer to the slot at cursor_index. Advanced alongside CAS.
    cursor_ptr: AtomicPtr<Slot<R>>,
    /// True once finalize has produced the result.
    done: AtomicBool,
}

// SAFETY: Heap access is serialized by the cursor CAS — only the thread
// that advances the cursor writes to the heap. H doesn't need Send because
// it never physically moves between threads — it's created on one thread
// and mutated by whichever thread wins the CAS (one at a time).
unsafe impl<H, R: Send> Send for SlotChain<H, R> {}
unsafe impl<H, R: Send> Sync for SlotChain<H, R> {}

impl<H, R> SlotChain<H, R> {
    pub fn new(heap: H) -> Self {
        SlotChain {
            heap: UnsafeCell::new(heap),
            result: UnsafeCell::new(None),
            head: AtomicPtr::new(std::ptr::null_mut()),
            tail: AtomicPtr::new(std::ptr::null_mut()),
            appended: AtomicU32::new(0),
            total: AtomicU32::new(0),
            total_known: AtomicBool::new(false),
            cursor_index: AtomicU32::new(0),
            cursor_ptr: AtomicPtr::new(std::ptr::null_mut()),
            done: AtomicBool::new(false),
        }
    }

    /// Append a slot for a new child. Returns a pointer to it.
    /// Called sequentially by the cursor-pulling thread (dispatch_rest).
    pub fn append_slot(&self) -> SlotPtr<R> {
        let slot = Box::into_raw(Box::new(Slot {
            result: UnsafeCell::new(MaybeUninit::uninit()),
            filled: AtomicBool::new(false),
            next: AtomicPtr::new(std::ptr::null_mut()),
        }));

        let old_tail = self.tail.swap(slot, Ordering::AcqRel);
        if old_tail.is_null() {
            self.head.store(slot, Ordering::Release);
            // Initialize cursor_ptr to head
            self.cursor_ptr.store(slot, Ordering::Release);
        } else {
            unsafe { (*old_tail).next.store(slot, Ordering::Release); }
        }
        self.appended.fetch_add(1, Ordering::Release);
        SlotPtr(slot)
    }

    /// Mark total children known. Calls try_advance.
    pub fn set_total<N>(&self, fold: &impl FoldOps<N, H, R>) {
        let total = self.appended.load(Ordering::Acquire);
        self.total.store(total, Ordering::Release);
        self.total_known.store(true, Ordering::Release);
        self.try_advance(fold);
    }

    /// Deliver a result to a slot. Calls try_advance.
    pub fn deliver<N>(&self, slot: SlotPtr<R>, result: R, fold: &impl FoldOps<N, H, R>) {
        let s = unsafe { &*slot.0 };
        unsafe { (*s.result.get()).write(result); }
        s.filled.store(true, Ordering::Release);
        self.try_advance(fold);
    }

    /// The single reactive driver. Advances the cursor through contiguous
    /// filled slots, accumulating each. Finalizes when cursor reaches total.
    fn try_advance<N>(&self, fold: &impl FoldOps<N, H, R>) {
        loop {
            let pos = self.cursor_index.load(Ordering::Acquire);

            // Check completion
            if self.total_known.load(Ordering::Acquire)
                && pos >= self.total.load(Ordering::Acquire)
            {
                // All accumulated. Finalize (only once).
                if self.done.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    let heap = unsafe { &*self.heap.get() };
                    let result = fold.finalize(heap);
                    unsafe { *self.result.get() = Some(result); }
                }
                return;
            }

            // Load the cursor slot pointer
            let slot_ptr = self.cursor_ptr.load(Ordering::Acquire);
            if slot_ptr.is_null() { return; } // no slots yet
            let slot = unsafe { &*slot_ptr };

            // Is this slot filled?
            if !slot.filled.load(Ordering::Acquire) { return; } // gap

            // CAS cursor_index: pos → pos+1. Winner accumulates.
            if self.cursor_index.compare_exchange(
                pos, pos + 1, Ordering::AcqRel, Ordering::Relaxed
            ).is_err() {
                continue; // another thread advanced, retry
            }

            // We own this position. Accumulate.
            let heap = unsafe { &mut *self.heap.get() };
            fold.accumulate(heap, unsafe { (*slot.result.get()).assume_init_ref() });

            // Advance cursor_ptr to next slot
            let next = slot.next.load(Ordering::Acquire);
            self.cursor_ptr.store(next, Ordering::Release);

            // Loop: try to advance further
        }
    }

    /// Called after all joins return. Returns the finalized result.
    /// At this point all deliveries + set_total have happened.
    /// try_advance may or may not have completed — call it once more.
    pub fn finish<N>(&self, fold: &impl FoldOps<N, H, R>) -> R {
        self.try_advance(fold);
        // Spin briefly if another thread is mid-finalize
        while !self.done.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        unsafe { (*self.result.get()).take().expect("chain result not set") }
    }
}

impl<H, R> Drop for SlotChain<H, R> {
    fn drop(&mut self) {
        let cursor = self.cursor_index.load(Ordering::Relaxed);
        let mut current = *self.head.get_mut();
        let mut index = 0u32;
        while !current.is_null() {
            let mut slot = unsafe { Box::from_raw(current) };
            // Slots past the cursor were filled but not accumulated —
            // drop their results. Slots before cursor were accumulated
            // (result was read but MaybeUninit still holds the bytes —
            // for types with drop, we already read via assume_init_ref
            // which doesn't move, so we need to drop here too).
            if *slot.filled.get_mut() && index >= cursor {
                unsafe { (*slot.result.get()).assume_init_drop(); }
            }
            current = *slot.next.get_mut();
            index += 1;
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
        chain.deliver::<()>(s0, 10, &SumFold); // fills gap, drains all
        assert_eq!(chain.finish::<()>(&SumFold), 60);
    }

    #[test]
    fn total_set_early() {
        let chain = SlotChain::new(0i32);
        let s0 = chain.append_slot();
        let s1 = chain.append_slot();
        chain.set_total::<()>(&SumFold); // total known before any delivery
        chain.deliver::<()>(s0, 10, &SumFold);
        chain.deliver::<()>(s1, 20, &SumFold);
        assert_eq!(chain.finish::<()>(&SumFold), 30);
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
