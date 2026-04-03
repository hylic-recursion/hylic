// Parallel execution infrastructure — clean reimplementation.
//
// Module structure:
//   unsafe_core/    — all unsafe primitives, encapsulated with safe APIs
//     slot.rs       — Slot<T>: inline storage + AtomicBool available flag
//     segment.rs    — Segment<T>: fixed-size array of Slots, lazily allocated
//     task_ref.rs   — TaskRef: type-erased pointer to a TaskSlot (raw pointer)
//   steal_queue.rs  — StealQueue<T>: segmented monotonic push+steal queue (safe API)
//   task_slot.rs    — TaskSlot<F,R>: fork point with stolen/done flags (safe API over unsafe_core)
//   pool.rs         — WorkPool + PoolExecView + ViewHandle (fully safe)
//   completion.rs   — Completion<R>: one-shot result slot (fully safe)
//   context_slot.rs — ContextSlot<T>: scoped injection (exists, clean)
//   lift/
//     lazy.rs       — ParLazy
//     eager.rs      — ParEager + Collector + EagerSpec
//     mod.rs
//
// Unsafe boundary:
//   unsafe_core/ is the ONLY module with `unsafe impl Send/Sync` and raw pointer dereference.
//   Everything above it uses safe Rust only.
//   The unsafe surface is: Slot (UnsafeCell + AtomicBool), Segment (AtomicPtr allocation),
//   TaskRef (raw pointer to stack-allocated TaskSlot).

pub mod context_slot;
pub(crate) mod unsafe_core;
pub mod steal_queue;
pub mod task_slot;
pub mod pool;

pub use context_slot::ContextSlot;
