//! Arena<N, H, R>: concurrent node registry for the hylomorphic executor.
//!
//! Stores node values (N), heaps (H), and results (R) in segmented
//! pre-allocated blocks. References to nodes are stable (segments never
//! move). This is the ONLY unsafe code in the hylomorphic executor.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};

const SEGMENT_SIZE: usize = 64;
const MAX_SEGMENTS: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NodeId(pub u32);

pub const PENDING: u8 = 0;
pub const ACTIVE: u8 = 1;
pub const READY: u8 = 2;

// ── NodeEntry ────────────────────────────────────────

/// A single node in the arena. Holds the node value, fold heap,
/// fold result, tree structure links, and scheduling state.
pub struct NodeEntry<N, H, R> {
    pub state: AtomicU8,
    node_value: UnsafeCell<MaybeUninit<N>>,
    heap: UnsafeCell<MaybeUninit<H>>,
    result: UnsafeCell<MaybeUninit<R>>,
    pub parent: Option<NodeId>,
    pub sibling_index: u32,
    children: UnsafeCell<Vec<NodeId>>,
    pub children_total: AtomicU32,
    pub children_done: AtomicU32,
    pub children_known: AtomicBool,
}

// SAFETY: node_value and result are Send (cross thread boundaries).
// heap is NOT required to be Send — it's only accessed by one thread
// at a time (Active worker writes, last-child runs finalize). The state
// machine enforces single-writer for heap. UnsafeCell makes this sound.
unsafe impl<N: Send, H, R: Send> Send for NodeEntry<N, H, R> {}
unsafe impl<N: Send, H, R: Send> Sync for NodeEntry<N, H, R> {}

impl<N, H, R> NodeEntry<N, H, R> {
    fn new(node_value: N, parent: Option<NodeId>, sibling_index: u32) -> Self {
        let mut nv = MaybeUninit::uninit();
        nv.write(node_value);
        NodeEntry {
            state: AtomicU8::new(PENDING),
            node_value: UnsafeCell::new(nv),
            heap: UnsafeCell::new(MaybeUninit::uninit()),
            result: UnsafeCell::new(MaybeUninit::uninit()),
            parent,
            sibling_index,
            children: UnsafeCell::new(Vec::new()),
            children_total: AtomicU32::new(0),
            children_done: AtomicU32::new(0),
            children_known: AtomicBool::new(false),
        }
    }

    pub fn try_activate(&self) -> bool {
        self.state.compare_exchange(PENDING, ACTIVE, Ordering::AcqRel, Ordering::Relaxed).is_ok()
    }

    /// Read the node value. Valid after allocation (always initialized).
    pub fn node_value(&self) -> &N {
        unsafe { (*self.node_value.get()).assume_init_ref() }
    }

    pub fn write_heap(&self, heap: H) {
        unsafe { (*self.heap.get()).write(heap); }
    }

    /// # Safety: caller must have exclusive access (Active, or last-child finalize).
    pub unsafe fn heap_mut(&self) -> &mut H {
        unsafe { (*self.heap.get()).assume_init_mut() }
    }

    pub fn write_result(&self, result: R) {
        unsafe { (*self.result.get()).write(result); }
        self.state.store(READY, Ordering::Release);
    }

    pub fn read_result(&self) -> &R {
        debug_assert_eq!(self.state.load(Ordering::Acquire), READY);
        unsafe { (*self.result.get()).assume_init_ref() }
    }

    pub fn take_result(&self) -> R {
        debug_assert_eq!(self.state.load(Ordering::Acquire), READY);
        unsafe { (*self.result.get()).assume_init_read() }
    }

    /// Append a child id. Only called by the Active worker during visit.
    pub fn push_child(&self, child: NodeId) {
        unsafe { (*self.children.get()).push(child); }
    }

    /// Get child by sibling index. Valid after children_known.
    pub fn child_at(&self, index: u32) -> NodeId {
        unsafe { (&(*self.children.get()))[index as usize] }
    }

    pub fn set_children_known(&self, total: u32) {
        self.children_total.store(total, Ordering::Release);
        self.children_known.store(true, Ordering::Release);
    }

    /// Increment children_done. Returns true if this was the last child.
    pub fn child_completed(&self) -> bool {
        let done = self.children_done.fetch_add(1, Ordering::AcqRel) + 1;
        self.children_known.load(Ordering::Acquire)
            && done >= self.children_total.load(Ordering::Acquire)
    }

    pub fn all_children_done(&self) -> bool {
        self.children_known.load(Ordering::Acquire)
            && self.children_done.load(Ordering::Acquire)
                >= self.children_total.load(Ordering::Acquire)
    }
}

// ── Segment ──────────────────────────────────────────

struct Segment<N, H, R> {
    /// Raw storage for SEGMENT_SIZE entries. Initialized on demand by alloc.
    storage: Box<[UnsafeCell<MaybeUninit<NodeEntry<N, H, R>>>]>,
}

impl<N, H, R> Segment<N, H, R> {
    fn new() -> Self {
        let storage: Vec<UnsafeCell<MaybeUninit<NodeEntry<N, H, R>>>> =
            (0..SEGMENT_SIZE).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
        Segment { storage: storage.into_boxed_slice() }
    }

    /// Write a new entry at the given slot. Must only be called once per slot.
    unsafe fn write(&self, slot: usize, entry: NodeEntry<N, H, R>) {
        unsafe { (*self.storage[slot].get()).write(entry); }
    }

    /// Read an entry at the given slot. Must only be called after write.
    fn get(&self, slot: usize) -> &NodeEntry<N, H, R> {
        unsafe { (*self.storage[slot].get()).assume_init_ref() }
    }
}

// ── Arena ────────────────────────────────────────────

pub struct Arena<N, H, R> {
    segments: Box<[AtomicPtr<Segment<N, H, R>>]>,
    len: AtomicU32,
}

unsafe impl<N: Send, H, R: Send> Send for Arena<N, H, R> {}
unsafe impl<N: Send, H, R: Send> Sync for Arena<N, H, R> {}

impl<N: Send, H, R: Send> Arena<N, H, R> {
    pub fn new() -> Self {
        let segments: Vec<AtomicPtr<Segment<N, H, R>>> =
            (0..MAX_SEGMENTS).map(|_| AtomicPtr::new(std::ptr::null_mut())).collect();
        Arena {
            segments: segments.into_boxed_slice(),
            len: AtomicU32::new(0),
        }
    }

    /// Allocate a new node with value. Returns its NodeId.
    pub fn alloc(&self, node_value: N, parent: Option<NodeId>, sibling_index: u32) -> NodeId {
        let id = self.len.fetch_add(1, Ordering::Relaxed);
        let seg_idx = id as usize / SEGMENT_SIZE;
        let slot_idx = id as usize % SEGMENT_SIZE;
        assert!(seg_idx < MAX_SEGMENTS, "arena overflow");

        let seg = self.ensure_segment(seg_idx);
        unsafe { seg.write(slot_idx, NodeEntry::new(node_value, parent, sibling_index)); }

        NodeId(id)
    }

    /// Get a node by id. Valid for the arena's lifetime.
    pub fn get(&self, id: NodeId) -> &NodeEntry<N, H, R> {
        let seg_idx = id.0 as usize / SEGMENT_SIZE;
        let slot_idx = id.0 as usize % SEGMENT_SIZE;
        let seg = unsafe { &*self.segments[seg_idx].load(Ordering::Acquire) };
        seg.get(slot_idx)
    }

    pub fn len(&self) -> u32 {
        self.len.load(Ordering::Acquire)
    }

    fn ensure_segment(&self, seg_idx: usize) -> &Segment<N, H, R> {
        let ptr = self.segments[seg_idx].load(Ordering::Acquire);
        if !ptr.is_null() {
            return unsafe { &*ptr };
        }
        let new = Box::into_raw(Box::new(Segment::new()));
        match self.segments[seg_idx].compare_exchange(
            std::ptr::null_mut(), new, Ordering::AcqRel, Ordering::Acquire,
        ) {
            Ok(_) => unsafe { &*new },
            Err(existing) => {
                unsafe { drop(Box::from_raw(new)); }
                unsafe { &*existing }
            }
        }
    }
}

impl<N, H, R> Drop for Arena<N, H, R> {
    fn drop(&mut self) {
        // Drop initialized entries, then segments.
        let total = *self.len.get_mut() as usize;
        for i in 0..total {
            let seg_idx = i / SEGMENT_SIZE;
            let slot_idx = i % SEGMENT_SIZE;
            let seg_ptr = *self.segments[seg_idx].get_mut();
            if !seg_ptr.is_null() {
                unsafe {
                    let seg = &*seg_ptr;
                    std::ptr::drop_in_place((*seg.storage[slot_idx].get()).as_mut_ptr());
                }
            }
        }
        for entry in self.segments.iter_mut() {
            let ptr = *entry.get_mut();
            if !ptr.is_null() {
                unsafe { drop(Box::from_raw(ptr)); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_read_value() {
        let arena: Arena<String, i32, i32> = Arena::new();
        let id = arena.alloc("hello".to_string(), None, 0);
        assert_eq!(arena.get(id).node_value(), "hello");
        assert_eq!(arena.get(id).state.load(Ordering::Relaxed), PENDING);
    }

    #[test]
    fn full_lifecycle() {
        let arena: Arena<i32, i32, i32> = Arena::new();
        let root = arena.alloc(100, None, 0);
        let c0 = arena.alloc(10, Some(root), 0);
        let c1 = arena.alloc(20, Some(root), 1);

        let r = arena.get(root);
        assert!(r.try_activate());
        r.write_heap(0);
        r.push_child(c0);
        r.push_child(c1);
        r.set_children_known(2);

        // Process c0
        let e0 = arena.get(c0);
        assert!(e0.try_activate());
        e0.write_heap(*e0.node_value());
        e0.set_children_known(0);
        e0.write_result(*e0.node_value());
        assert!(!r.child_completed()); // not last

        // Process c1
        let e1 = arena.get(c1);
        assert!(e1.try_activate());
        e1.write_heap(*e1.node_value());
        e1.set_children_known(0);
        e1.write_result(*e1.node_value());
        assert!(r.child_completed()); // last!

        // Finalize root
        let heap = unsafe { r.heap_mut() };
        *heap += e0.read_result();
        *heap += e1.read_result();
        r.write_result(*heap);
        assert_eq!(r.take_result(), 30);
    }

    #[test]
    fn cross_segment() {
        let arena: Arena<u32, u32, u32> = Arena::new();
        for i in 0..200u32 {
            let id = arena.alloc(i, None, 0);
            let e = arena.get(id);
            assert!(e.try_activate());
            e.write_heap(i);
            e.set_children_known(0);
            e.write_result(i);
        }
        for i in 0..200u32 {
            assert_eq!(*arena.get(NodeId(i)).read_result(), i);
        }
    }

    #[test]
    fn concurrent_alloc() {
        use std::sync::{Arc, Barrier};
        let arena = Arc::new(Arena::<u32, u32, u32>::new());
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4).map(|t| {
            let a = arena.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                for i in 0..100u32 {
                    let id = a.alloc(t * 100 + i, None, 0);
                    assert_eq!(*a.get(id).node_value(), t * 100 + i);
                }
            })
        }).collect();
        for h in handles { h.join().unwrap(); }
        assert_eq!(arena.len(), 400);
    }
}
