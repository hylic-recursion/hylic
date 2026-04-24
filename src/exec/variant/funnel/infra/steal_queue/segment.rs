//! Segment<T> + SegmentTable<T>: lazily-allocated fixed-size blocks.
//!
//! A SegmentTable maps monotonically-increasing positions to Slot<T>
//! entries across a chain of fixed-size Segments. Segments are allocated
//! lazily on first access and never freed until the table drops.
//!
//! This is the only place AtomicPtr + Box::into_raw/from_raw appears.
//! The safe API: `get_slot(pos)` returns `&Slot<T>`, allocating the
//! segment if needed. Concurrent `get_slot` calls for the same segment
//! race on the AtomicPtr — one allocates, losers free their allocation.

use std::sync::atomic::{AtomicPtr, Ordering};
use super::slot::Slot;

/// Number of slots per segment. Power of 2 for masking.
pub const SEGMENT_SIZE: usize = 64;

/// A fixed-size array of Slots.
pub struct Segment<T> {
    slots: Box<[Slot<T>]>,
}

impl<T> Segment<T> {
    fn new() -> Self {
        let slots: Vec<Slot<T>> = (0..SEGMENT_SIZE).map(|_| Slot::new()).collect();
        Segment { slots: slots.into_boxed_slice() }
    }

    pub fn slot(&self, index: usize) -> &Slot<T> {
        &self.slots[index]
    }
}

/// Table of lazily-allocated segments. Maps position → &Slot<T>.
///
/// Thread-safe: concurrent `get_slot` calls for the same position
/// are safe — exactly one thread allocates the segment (CAS), losers
/// discard their allocation.
pub struct SegmentTable<T> {
    /// Array of AtomicPtr to segments. Index = position / SEGMENT_SIZE.
    /// Null = not yet allocated. Non-null = heap-allocated Segment.
    table: Box<[AtomicPtr<Segment<T>>]>,
}

// SAFETY: Each AtomicPtr either points to a heap-allocated Segment
// (owned by the table, freed on drop) or is null. Segments contain
// Slot<T> which is Send+Sync. AtomicPtr is Send+Sync.
unsafe impl<T: Send> Send for SegmentTable<T> {}
unsafe impl<T: Send> Sync for SegmentTable<T> {}

/// Maximum number of segments. With SEGMENT_SIZE=64, this allows
/// 64 * 4096 = 262144 positions — more than enough for any tree fold.
const MAX_SEGMENTS: usize = 4096;

impl<T> SegmentTable<T> {
    pub fn new() -> Self {
        let table: Vec<AtomicPtr<Segment<T>>> =
            (0..MAX_SEGMENTS).map(|_| AtomicPtr::new(std::ptr::null_mut())).collect();
        SegmentTable { table: table.into_boxed_slice() }
    }

    /// Get the slot at the given global position.
    /// Allocates the segment lazily if it doesn't exist.
    ///
    /// Panics if position exceeds MAX_SEGMENTS * SEGMENT_SIZE.
    pub fn get_slot(&self, pos: u64) -> &Slot<T> {
        let seg_idx = (pos as usize) / SEGMENT_SIZE;
        let slot_idx = (pos as usize) % SEGMENT_SIZE;
        assert!(seg_idx < MAX_SEGMENTS, "position {pos} exceeds segment table capacity");
        let segment = self.ensure_segment(seg_idx);
        segment.slot(slot_idx)
    }

    /// Ensure the segment at `seg_idx` is allocated. Returns a reference.
    fn ensure_segment(&self, seg_idx: usize) -> &Segment<T> {
        let ptr = self.table[seg_idx].load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: installed segments live for &self's lifetime —
            // freed only in Drop via &mut self.
            return unsafe { &*ptr };
        }

        // Segment not yet allocated. Allocate and try to install.
        let new_seg = Box::new(Segment::new());
        let new_ptr = Box::into_raw(new_seg);

        match self.table[seg_idx].compare_exchange(
            std::ptr::null_mut(),
            new_ptr,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // SAFETY: the CAS transferred ownership of new_ptr to
                // `self.table[seg_idx]`; the allocation outlives self.
                unsafe { &*new_ptr }
            }
            Err(existing) => {
                // SAFETY (drop): CAS failed, so new_ptr is still owned
                // by us and can be freed. (existing): the winner's
                // allocation is owned by the table and live for &self.
                unsafe { drop(Box::from_raw(new_ptr)); }
                unsafe { &*existing }
            }
        }
    }
}

impl<T> Drop for SegmentTable<T> {
    fn drop(&mut self) {
        for entry in self.table.iter_mut() {
            let ptr = *entry.get_mut();
            if !ptr.is_null() {
                // SAFETY: non-null segment pointers came from
                // Box::into_raw in ensure_segment. &mut self gives us
                // exclusive access during drop.
                unsafe { drop(Box::from_raw(ptr)); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // SAFETY (throughout this module): write() is called on an EMPTY
    // slot by the test (single-writer), and read() is called after a
    // successful try_steal that transitions the slot to STOLEN —
    // matching the Slot's documented protocol.
    use super::*;
    use crate::exec::funnel::infra::steal_queue::slot as slot_mod;
    use std::sync::{Arc, Barrier};

    #[test]
    fn basic_get_slot() {
        let table = SegmentTable::<i32>::new();
        let slot = table.get_slot(0);
        assert_eq!(slot.state(), slot_mod::EMPTY);
        unsafe { slot.write(42); }
        assert_eq!(slot.state(), slot_mod::AVAILABLE);
        assert!(slot.try_steal());
        assert_eq!(slot.state(), slot_mod::STOLEN);
        assert_eq!(unsafe { slot.read() }, 42);
    }

    #[test]
    fn cross_segment_access() {
        let table = SegmentTable::<i32>::new();
        // Write to positions across multiple segments
        for i in 0..200u64 {
            let slot = table.get_slot(i);
            unsafe { slot.write(i as i32); }
        }
        // Read them back
        for i in 0..200u64 {
            let slot = table.get_slot(i);
            assert!(slot.try_steal());
            assert_eq!(unsafe { slot.read() }, i as i32);
        }
    }

    #[test]
    fn segment_reuse() {
        let table = SegmentTable::<i32>::new();
        // Access positions 0 and 1 — same segment
        let s0 = table.get_slot(0) as *const _;
        let s1 = table.get_slot(1) as *const _;
        // Different slots but same segment
        assert_ne!(s0, s1);
        // Access position SEGMENT_SIZE — different segment
        let s64 = table.get_slot(SEGMENT_SIZE as u64) as *const _;
        assert_ne!(s0, s64);
    }

    #[test]
    fn concurrent_segment_allocation() {
        // 4 threads all access position 0 simultaneously.
        // Only one segment should be allocated.
        for _ in 0..50 {
            let table = Arc::new(SegmentTable::<i32>::new());
            let barrier = Arc::new(Barrier::new(4));

            let handles: Vec<_> = (0..4).map(|_| {
                let t = table.clone();
                let b = barrier.clone();
                std::thread::spawn(move || {
                    b.wait();
                    let slot = t.get_slot(0);
                    slot as *const _ as usize
                })
            }).collect();

            let addrs: Vec<usize> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            // All threads got the same slot address
            assert!(addrs.iter().all(|&a| a == addrs[0]),
                "different slot addresses: {:?}", addrs);
        }
    }

    #[test]
    fn concurrent_different_segments() {
        // 4 threads access 4 different segments simultaneously.
        let table = Arc::new(SegmentTable::<i32>::new());
        let barrier = Arc::new(Barrier::new(4));

        let handles: Vec<_> = (0..4).map(|t| {
            let table = table.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                let pos = (t as u64) * (SEGMENT_SIZE as u64);
                let slot = table.get_slot(pos);
                unsafe { slot.write(t as i32 * 100); }
            })
        }).collect();

        for h in handles { h.join().unwrap(); }

        // Verify all 4 segments have correct values
        for t in 0..4 {
            let pos = (t as u64) * (SEGMENT_SIZE as u64);
            let slot = table.get_slot(pos);
            assert!(slot.try_steal());
            assert_eq!(unsafe { slot.read() }, t as i32 * 100);
        }
    }
}
