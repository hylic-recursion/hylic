//! SegmentedSlab<T>: generic growable bump allocator with stable references.
//!
//! Lazily-allocated fixed-size segments. Segments are heap-allocated on
//! first access via AtomicPtr CAS — one thread installs, losers free
//! their allocation. Once a segment exists its address never changes,
//! so references into a segment are stable across concurrent alloc()
//! calls. This is the key invariant that makes it safe under the
//! walk_cps recursion pattern where alloc() and get_ref() interleave
//! with live references.
//!
//! Zero-cost wrapper target: Arena<T> and ContArena<T> are thin
//! newtypes over SegmentedSlab<T> differing only in which operations
//! they expose and their Drop behavior.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

/// Slots per segment. Power of 2 for shift+mask decomposition.
const SEGMENT_BITS: u32 = 6;
const SEGMENT_SIZE: usize = 1 << SEGMENT_BITS;     // 64
const SEGMENT_MASK: usize = SEGMENT_SIZE - 1;       // 0x3F

/// Maximum number of segments. Total capacity: 64 × 4096 = 262,144.
const MAX_SEGMENTS: usize = 4096;

// ── Segment ─────────────────────────────────────────

struct Segment<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
}

impl<T> Segment<T> {
    fn new() -> Self {
        let slots: Vec<UnsafeCell<MaybeUninit<T>>> =
            (0..SEGMENT_SIZE).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
        Segment { slots: slots.into_boxed_slice() }
    }

    #[inline(always)]
    fn slot(&self, offset: usize) -> &UnsafeCell<MaybeUninit<T>> {
        &self.slots[offset]
    }
}

// ── Index decomposition ─────────────────────────────

#[inline(always)]
fn segment_of(idx: u32) -> usize { (idx >> SEGMENT_BITS) as usize }

#[inline(always)]
fn offset_of(idx: u32) -> usize { (idx as usize) & SEGMENT_MASK }

// ── SegmentedSlab ───────────────────────────────────

pub(crate) struct SegmentedSlab<T> {
    segments: Box<[AtomicPtr<Segment<T>>]>,
    next: AtomicU32,
}

// SAFETY: Each AtomicPtr either points to a heap-allocated Segment
// (owned by the slab, freed on drop) or is null. UnsafeCell<MaybeUninit<T>>
// slots follow the same safety model as Arena — alloc writes before
// returning the index, get_ref/take are called only on allocated slots.
unsafe impl<T: Send> Send for SegmentedSlab<T> {}
unsafe impl<T: Send + Sync> Sync for SegmentedSlab<T> {}

impl<T> SegmentedSlab<T> {
    pub fn new() -> Self {
        let segments: Vec<AtomicPtr<Segment<T>>> =
            (0..MAX_SEGMENTS).map(|_| AtomicPtr::new(std::ptr::null_mut())).collect();
        SegmentedSlab {
            segments: segments.into_boxed_slice(),
            next: AtomicU32::new(0),
        }
    }

    /// Bump-allocate a value, returning its linear index.
    /// Lazily allocates the target segment if needed.
    #[inline]
    pub fn alloc(&self, value: T) -> u32 {
        let idx = self.next.fetch_add(1, Ordering::Relaxed);
        let seg_idx = segment_of(idx);
        debug_assert!(seg_idx < MAX_SEGMENTS, "segmented slab exhausted: {} elements", idx);
        let segment = self.ensure_segment(seg_idx);
        unsafe { (*segment.slot(offset_of(idx)).get()).write(value); }
        idx
    }

    /// Get a shared reference to the value at `idx`.
    ///
    /// # Safety
    /// The slot must have been previously allocated via `alloc`.
    #[inline]
    pub unsafe fn get_ref(&self, idx: u32) -> &T {
        let ptr = self.segments[segment_of(idx)].load(Ordering::Acquire);
        debug_assert!(!ptr.is_null());
        unsafe { (*(*ptr).slot(offset_of(idx)).get()).assume_init_ref() }
    }

    /// Move the value out of the slot (slot becomes uninit).
    ///
    /// # Safety
    /// Must be called exactly once per allocated slot.
    #[inline]
    pub unsafe fn take(&self, idx: u32) -> T {
        let ptr = self.segments[segment_of(idx)].load(Ordering::Acquire);
        debug_assert!(!ptr.is_null());
        unsafe { (*(*ptr).slot(offset_of(idx)).get()).assume_init_read() }
    }

    /// Drop all values in allocated slots.
    /// Called by Arena's Drop (where values are never moved out).
    ///
    /// # Safety
    /// Must only be called during drop (exclusive access, &mut self).
    /// All slots 0..count must contain initialized values.
    pub fn drop_allocated_values(&mut self) {
        let count = *self.next.get_mut() as usize;
        for i in 0..count {
            let seg_idx = segment_of(i as u32);
            let off = offset_of(i as u32);
            let ptr = *self.segments[seg_idx].get_mut();
            debug_assert!(!ptr.is_null());
            unsafe { (*(*ptr).slot(off).get()).assume_init_drop(); }
        }
    }

    /// Free all heap-allocated segments.
    /// Called by both Arena and ContArena Drop impls after any
    /// value cleanup.
    pub fn drop_segments(&mut self) {
        for entry in self.segments.iter_mut() {
            let ptr = *entry.get_mut();
            if !ptr.is_null() {
                unsafe { drop(Box::from_raw(ptr)); }
            }
        }
    }

    /// Ensure the segment at `seg_idx` is allocated. Returns a reference.
    /// Concurrent calls for the same segment race on the AtomicPtr —
    /// exactly one thread installs its allocation, losers free theirs.
    #[inline]
    fn ensure_segment(&self, seg_idx: usize) -> &Segment<T> {
        let ptr = self.segments[seg_idx].load(Ordering::Acquire);
        if !ptr.is_null() {
            return unsafe { &*ptr };
        }
        self.ensure_segment_slow(seg_idx)
    }

    #[cold]
    fn ensure_segment_slow(&self, seg_idx: usize) -> &Segment<T> {
        let new_seg = Box::into_raw(Box::new(Segment::new()));
        match self.segments[seg_idx].compare_exchange(
            std::ptr::null_mut(),
            new_seg,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => unsafe { &*new_seg },
            Err(existing) => {
                unsafe { drop(Box::from_raw(new_seg)); }
                unsafe { &*existing }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_alloc_and_get() {
        let slab = SegmentedSlab::new();
        let i0 = slab.alloc(42);
        let i1 = slab.alloc(99);
        assert_eq!(unsafe { *slab.get_ref(i0) }, 42);
        assert_eq!(unsafe { *slab.get_ref(i1) }, 99);
    }

    #[test]
    fn alloc_and_take() {
        let slab = SegmentedSlab::new();
        let i0 = slab.alloc(String::from("hello"));
        let i1 = slab.alloc(String::from("world"));
        assert_eq!(unsafe { slab.take(i0) }, "hello");
        assert_eq!(unsafe { slab.take(i1) }, "world");
    }

    #[test]
    fn cross_segment_alloc() {
        let slab = SegmentedSlab::new();
        let mut indices = Vec::new();
        // Allocate across 4 segments (256 elements)
        for i in 0..256u32 {
            indices.push(slab.alloc(i));
        }
        for (i, &idx) in indices.iter().enumerate() {
            assert_eq!(unsafe { *slab.get_ref(idx) }, i as u32);
        }
    }

    #[test]
    fn concurrent_alloc() {
        let slab = std::sync::Arc::new(SegmentedSlab::new());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let handles: Vec<_> = (0..4).map(|t| {
            let s = slab.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                let mut indices = Vec::new();
                for i in 0..250 {
                    indices.push(s.alloc(t * 1000 + i));
                }
                indices
            })
        }).collect();
        let all: Vec<Vec<u32>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        for (t, indices) in all.iter().enumerate() {
            for (i, &idx) in indices.iter().enumerate() {
                assert_eq!(unsafe { *slab.get_ref(idx) }, t * 1000 + i);
            }
        }
    }

    #[test]
    fn concurrent_segment_allocation() {
        // 4 threads all allocate into a fresh slab simultaneously.
        // The first allocs from each thread may race on segment 0.
        for _ in 0..50 {
            let slab = std::sync::Arc::new(SegmentedSlab::new());
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
            let handles: Vec<_> = (0..4).map(|t| {
                let s = slab.clone();
                let b = barrier.clone();
                std::thread::spawn(move || {
                    b.wait();
                    s.alloc(t as i32)
                })
            }).collect();
            let indices: Vec<u32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            // All indices should be distinct
            let mut sorted = indices.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), 4, "duplicate indices: {:?}", indices);
        }
    }

    #[test]
    fn drop_values() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
        struct Dropper;
        impl Drop for Dropper {
            fn drop(&mut self) { DROP_COUNT.fetch_add(1, Ordering::Relaxed); }
        }
        DROP_COUNT.store(0, Ordering::Relaxed);
        {
            let mut slab = SegmentedSlab::new();
            slab.alloc(Dropper);
            slab.alloc(Dropper);
            slab.alloc(Dropper);
            slab.drop_allocated_values();
            slab.drop_segments();
            // Prevent SegmentedSlab's own drop from running (we already cleaned up)
            std::mem::forget(slab);
        }
        assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn stable_references_across_alloc() {
        // The key invariant: get_ref returns stable references that
        // survive subsequent alloc calls (including cross-segment).
        let slab = SegmentedSlab::new();
        let i0 = slab.alloc(42u64);
        let r0 = unsafe { slab.get_ref(i0) };
        // Allocate enough to trigger new segment allocation
        for i in 1..200 {
            slab.alloc(i);
        }
        // Original reference must still be valid
        assert_eq!(*r0, 42);
    }
}
