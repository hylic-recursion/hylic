//! Arena<H, R>: concurrent node registry for the hylomorphic executor.
//!
//! Nodes are allocated in fixed-size segments (like StealQueue). Segments
//! never move after allocation — references to nodes are stable. The
//! segment table uses AtomicPtr for lazy allocation (same pattern as
//! StealQueue's SegmentTable).
//!
//! This is the ONLY unsafe code in the hylomorphic executor.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};

const SEGMENT_SIZE: usize = 64;
const MAX_SEGMENTS: usize = 4096; // 4096 * 64 = 262144 nodes max

// ── NodeId ───────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NodeId(pub u32);

// ── Node states ──────────────────────────────────────

pub const PENDING: u8 = 0;
pub const ACTIVE: u8 = 1;
pub const READY: u8 = 2;

// ── NodeEntry ────────────────────────────────────────

pub struct NodeEntry<H, R> {
    pub state: AtomicU8,
    heap: UnsafeCell<MaybeUninit<H>>,
    result: UnsafeCell<MaybeUninit<R>>,
    pub parent: Option<NodeId>,
    pub sibling_index: u32,
    /// Children discovered so far. Protected by: only the Active worker
    /// appends (during visit), readers wait until children_known.
    children: UnsafeCell<Vec<NodeId>>,
    pub children_total: AtomicU32,
    pub children_done: AtomicU32,
    pub children_known: AtomicBool,
}

unsafe impl<H: Send, R: Send> Send for NodeEntry<H, R> {}
unsafe impl<H: Send, R: Send> Sync for NodeEntry<H, R> {}

impl<H, R> NodeEntry<H, R> {
    fn new(parent: Option<NodeId>, sibling_index: u32) -> Self {
        NodeEntry {
            state: AtomicU8::new(PENDING),
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

    pub fn write_heap(&self, heap: H) {
        unsafe { (*self.heap.get()).write(heap); }
    }

    /// # Safety: caller must have exclusive access (Active worker or last-child finalize).
    pub unsafe fn heap_mut(&self) -> &mut H {
        unsafe { (*self.heap.get()).assume_init_mut() }
    }

    pub fn write_result(&self, result: R) {
        unsafe { (*self.result.get()).write(result); }
        self.state.store(READY, Ordering::Release);
    }

    pub fn take_result(&self) -> R {
        debug_assert_eq!(self.state.load(Ordering::Acquire), READY);
        unsafe { (*self.result.get()).assume_init_read() }
    }

    pub fn read_result(&self) -> &R {
        debug_assert_eq!(self.state.load(Ordering::Acquire), READY);
        unsafe { (*self.result.get()).assume_init_ref() }
    }

    /// Append a child. Only called by the Active worker during visit.
    pub fn push_child(&self, child: NodeId) {
        unsafe { (*self.children.get()).push(child); }
    }

    /// Get child by sibling index. Only valid after children_known.
    pub fn child_at(&self, index: u32) -> NodeId {
        unsafe { (*self.children.get())[index as usize] }
    }

    pub fn set_children_known(&self, total: u32) {
        self.children_total.store(total, Ordering::Release);
        self.children_known.store(true, Ordering::Release);
    }

    /// Increment children_done. Returns true if this was the last child
    /// AND children_known is set.
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

struct Segment<H, R> {
    nodes: Box<[NodeEntry<H, R>]>,
}

impl<H, R> Segment<H, R> {
    fn new_uninit() -> Self {
        // Allocate SEGMENT_SIZE entries. They start uninitialized —
        // each will be initialized by Arena::alloc via placement.
        // We use a trick: allocate with dummy values that will be
        // overwritten. NodeEntry::new creates valid atomic state.
        let nodes: Vec<NodeEntry<H, R>> = (0..SEGMENT_SIZE)
            .map(|_| NodeEntry::new(None, 0))
            .collect();
        Segment { nodes: nodes.into_boxed_slice() }
    }
}

// ── Arena ────────────────────────────────────────────

pub struct Arena<H, R> {
    segments: Box<[AtomicPtr<Segment<H, R>>]>,
    len: AtomicU32,
}

unsafe impl<H: Send, R: Send> Send for Arena<H, R> {}
unsafe impl<H: Send, R: Send> Sync for Arena<H, R> {}

impl<H: Send, R: Send> Arena<H, R> {
    pub fn new() -> Self {
        let segments: Vec<AtomicPtr<Segment<H, R>>> =
            (0..MAX_SEGMENTS).map(|_| AtomicPtr::new(std::ptr::null_mut())).collect();
        Arena {
            segments: segments.into_boxed_slice(),
            len: AtomicU32::new(0),
        }
    }

    pub fn alloc(&self, parent: Option<NodeId>, sibling_index: u32) -> NodeId {
        let id = self.len.fetch_add(1, Ordering::Relaxed);
        let seg_idx = id as usize / SEGMENT_SIZE;
        let slot_idx = id as usize % SEGMENT_SIZE;

        assert!(seg_idx < MAX_SEGMENTS, "arena overflow: {} nodes", id);

        // Ensure segment exists
        let seg = self.ensure_segment(seg_idx);

        // Initialize the slot (overwrite the dummy NodeEntry)
        let entry = &seg.nodes[slot_idx];
        entry.state.store(PENDING, Ordering::Relaxed);
        entry.parent = parent; // This is a data race if concurrent — but
        // alloc returns a unique id via fetch_add, so no two threads
        // initialize the same slot. WAIT: `parent` is not atomic.
        // We're writing to a non-atomic field. This is only safe because
        // we just allocated this slot — no other thread knows its id yet
        // (we haven't returned it). The fetch_add on `len` happens-before
        // this write, and the caller publishes the NodeId after alloc returns.

        // Actually, the Segment was pre-initialized with dummy NodeEntries.
        // We need to RE-initialize this specific slot. But NodeEntry fields
        // like `parent` are not atomic. Writing to them is safe only if no
        // other thread reads them concurrently. Since the NodeId hasn't been
        // published yet, this is safe.
        //
        // BUT: the segment is shared. Other threads might be reading
        // DIFFERENT slots in the same segment. That's fine — different
        // memory locations. The only concern is the slot at `slot_idx`,
        // which nobody else knows about yet.
        unsafe {
            let entry_ptr = &seg.nodes[slot_idx] as *const NodeEntry<H, R> as *mut NodeEntry<H, R>;
            std::ptr::write(entry_ptr, NodeEntry::new(parent, sibling_index));
        }

        NodeId(id)
    }

    /// Get a node by id. The node must have been allocated (id < len).
    /// Returns a reference that's valid for the arena's lifetime.
    pub fn get(&self, id: NodeId) -> &NodeEntry<H, R> {
        let seg_idx = id.0 as usize / SEGMENT_SIZE;
        let slot_idx = id.0 as usize % SEGMENT_SIZE;
        let seg_ptr = self.segments[seg_idx].load(Ordering::Acquire);
        debug_assert!(!seg_ptr.is_null(), "segment not allocated for NodeId {}", id.0);
        unsafe { &(*seg_ptr).nodes[slot_idx] }
    }

    fn ensure_segment(&self, seg_idx: usize) -> &Segment<H, R> {
        let ptr = self.segments[seg_idx].load(Ordering::Acquire);
        if !ptr.is_null() {
            return unsafe { &*ptr };
        }
        let new_seg = Box::new(Segment::new_uninit());
        let new_ptr = Box::into_raw(new_seg);
        match self.segments[seg_idx].compare_exchange(
            std::ptr::null_mut(), new_ptr,
            Ordering::AcqRel, Ordering::Acquire,
        ) {
            Ok(_) => unsafe { &*new_ptr },
            Err(existing) => {
                unsafe { drop(Box::from_raw(new_ptr)); }
                unsafe { &*existing }
            }
        }
    }
}

impl<H, R> Drop for Arena<H, R> {
    fn drop(&mut self) {
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
    fn alloc_and_get() {
        let arena: Arena<i32, i32> = Arena::new();
        let root = arena.alloc(None, 0);
        let child = arena.alloc(Some(root), 0);

        let r = arena.get(root);
        assert_eq!(r.state.load(Ordering::Relaxed), PENDING);
        assert!(r.try_activate());
        r.write_heap(42);

        let c = arena.get(child);
        assert!(c.try_activate());
        assert_eq!(c.parent, Some(root));
        assert_eq!(c.sibling_index, 0);
    }

    #[test]
    fn children_lifecycle() {
        let arena: Arena<i32, i32> = Arena::new();
        let parent = arena.alloc(None, 0);
        let p = arena.get(parent);
        p.try_activate();
        p.write_heap(0);

        let c0 = arena.alloc(Some(parent), 0);
        let c1 = arena.alloc(Some(parent), 1);
        p.push_child(c0);
        p.push_child(c1);
        p.set_children_known(2);

        assert_eq!(p.child_at(0), c0);
        assert_eq!(p.child_at(1), c1);
        assert!(!p.all_children_done());

        // Complete child 0
        let c0e = arena.get(c0);
        c0e.try_activate();
        c0e.write_heap(10);
        c0e.set_children_known(0);
        c0e.write_result(10);
        assert!(!p.child_completed()); // first child, not last

        // Complete child 1
        let c1e = arena.get(c1);
        c1e.try_activate();
        c1e.write_heap(20);
        c1e.set_children_known(0);
        c1e.write_result(20);
        assert!(p.child_completed()); // last child!
        assert!(p.all_children_done());
    }

    #[test]
    fn cross_segment() {
        let arena: Arena<i32, i32> = Arena::new();
        for i in 0..200u32 {
            let id = arena.alloc(None, i);
            let entry = arena.get(id);
            assert!(entry.try_activate());
            entry.write_heap(i as i32);
            entry.set_children_known(0);
            entry.write_result(i as i32);
        }
        for i in 0..200u32 {
            assert_eq!(*arena.get(NodeId(i)).read_result(), i as i32);
        }
    }

    #[test]
    fn concurrent_alloc() {
        use std::sync::{Arc, Barrier};
        let arena = Arc::new(Arena::<i32, i32>::new());
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4).map(|_| {
            let a = arena.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                for _ in 0..100 { a.alloc(None, 0); }
            })
        }).collect();
        for h in handles { h.join().unwrap(); }
        assert_eq!(arena.len.load(Ordering::Relaxed), 400);
    }
}
