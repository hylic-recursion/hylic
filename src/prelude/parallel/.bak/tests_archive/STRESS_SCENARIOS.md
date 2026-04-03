# Critical stress scenarios to preserve in the new implementation

## 1. Push/pop race (the bug we found)
- Multiple threads push while another thread pops from the same deque
- Root cause: `fetch_add` on bottom vs `CAS` decrement on bottom — different threads going in different directions
- In new design: ELIMINATED by removing pop entirely. Bottom only goes up.
- Regression test: `sequential_pool_fork_join_map_stress` — 20 iterations of pool creation → fork_join_map(64 items, 3 workers) → teardown

## 2. Last-item race (pop vs steal on single element)
- Chase-Lev resolution: SeqCst CAS on top as tiebreaker
- In new design: resolved by per-slot `stolen` CAS — publisher and worker race on the slot, not on deque indices
- Regression test: push one item, race reclaim vs steal from two threads, verify exactly one wins

## 3. Concurrent resize corruption
- Pusher holds stale buffer pointer while resizer swaps buffer
- In new design: ELIMINATED. Segmented queue — no resize, no buffer swap, no copy.
- Regression test: push more than SEGMENT_SIZE items to trigger new segment allocation under contention

## 4. Sequential pool reuse + concurrent pools
- Global thread_local WORKER_LOCAL leaked between pools
- In new design: ELIMINATED. No thread_local, no global state. Arc-based ViewHandle.
- Regression test: 20 iterations of pool create → fork_join_map → destroy. Also: two pools on separate threads simultaneously.

## 5. ParEager phase 1/2 overlap
- Leaf finalize submits tasks during Phase 1 (fused traversal). Workers process them (Phase 2) concurrently.
- Requires: the View's deque is active during lift_fold → Phase 1 → unwrap.
- Regression test: ParEager with PoolIn executor on 60-node tree, verify correct result

## 6. ViewHandle move-after-create
- Raw pointer to stack-local PoolExecView captured before the view was moved into an Rc stash
- In new design: ELIMINATED. Arc-based ViewHandle — stable heap address.
- Regression test: validated by the sequential_pool_fork_join_map_stress test

## 7. Multi-producer push correctness
- 4 threads push 500 items each = 2000 total. Verify all 2000 present, no duplicates.
- New design: fetch_add on bottom gives each pusher a unique position. Write to inline slot. Set available.
- Regression test: multi-producer, drain via steal, verify count and uniqueness.
