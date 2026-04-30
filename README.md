# hylic

A Rust library for catamorphic computation over trees and DAGs. The recursion does four things at each node: `init` builds a per-node heap, `visit` yields each child, each child's result is recursively computed and `accumulate`d into the heap, and `finalize` closes the heap into the node's result. hylic packages that pattern as three independent values: a `Fold<N, H, R>` carrying the closures, a `Treeish<N>` supplying children, and an executor that drives the recursion.

```rust
use hylic::prelude::*;

#[derive(Clone)]
struct Dir { size: u64, children: Vec<Dir> }

let f = fold(|d: &Dir| d.size,
             |heap: &mut u64, child: &u64| *heap += child,
             |h: &u64| *h);
let g = treeish(|d: &Dir| d.children.clone());

let total = FUSED.run(&f, &g, &dir);                          // sequential
let total = exec(funnel::Spec::default(4)).run(&f, &g, &dir); // parallel
```

The `Fold` and `Treeish` carry no reference to each other; the executor pairs them at the call site. Closures are stored behind `Arc` (Shared, Send+Sync), `Rc` (Local, single-thread), or `Box` (Owned, single-shot), and the executor is generic over the choice. Combinators on Fold and Treeish (`map`, `contramap`, `filter`, `memoize_treeish`) return new values that delegate to the originals; nothing is copied or rewritten.

A user-defined struct that implements `TreeOps<N>` (one method, `visit`) is callable from any executor without going through `Treeish`. The cookbook's `zero_cost_treeops` example uses this for an adjacency-list graph.

## Funnel

The parallel executor, `Funnel`, is bundled in this crate. Each policy preset is a triple of static types: a queue topology (per-worker deques or one shared FIFO), an accumulation strategy (fold each child's result into the parent's heap on arrival, or buffer until all siblings finish), and a wake policy (every push, once per batch, or every Kth push). The three are generic parameters on `Funnel<P>`, so picking a preset compiles a different specialisation of the entire walk; there is no runtime dispatch on strategy. Continuations are defunctionalised (a three-variant enum, not boxed closures) and live in arenas released at end of pool lifetime; the inner loop is `match cont` with no per-step allocation.

On the published 14-workload [Matrix benchmark](https://hylic-recursion.github.io/hylic-docs/cookbook/benchmarks.html#matrix), a `Funnel` variant wins ten rows outright against a handrolled Rayon recursion and a scoped pool, and lands within a few percent of the winner on the rest. Different presets win different rows: shallow-wide workloads prefer `Shared` queues with `OnArrival`, deep-narrow prefer `PerWorker` with `OnFinalize`. The interactive viewer at the link above marginalises on any axis.

## Lifts

A `Lift<D, N, H, R>` rewrites both the `Fold` and `Treeish` of a recursion in lockstep, possibly changing the `N`, `H`, or `R` carriers. The library ships per-axis lifts (`wrap_init_lift`, `zipmap_lift`, `filter_edges_lift`, `n_lift`, `explainer_lift`, …) and stacks them with `ComposedLift<L1, L2>`. Every composition monomorphises end to end. The [lifts chapter](https://hylic-recursion.github.io/hylic-docs/concepts/lifts.html) covers the trait surface and writing custom lifts.

For projects that prefer chained methods over composing lifts and slot values directly, [`hylic-pipeline`](https://github.com/hylic-recursion/hylic-pipeline) is a typestate builder over the same primitives.

## Documentation

<https://hylic-recursion.github.io/hylic-docs> is the long-form site. The [quick start](https://hylic-recursion.github.io/hylic-docs/quickstart.html) is a complete fold in three closures. [The recursive pattern](https://hylic-recursion.github.io/hylic-docs/concepts/separation.html) walks through the recursion engine and the trait surface around it. The [Funnel deep-dive](https://hylic-recursion.github.io/hylic-docs/funnel/overview.html) covers the CPS walk, the ticket system, and the three policy axes, with per-axis chapters on [queue topology](https://hylic-recursion.github.io/hylic-docs/funnel/queue_strategies.html), [accumulation](https://hylic-recursion.github.io/hylic-docs/funnel/accumulation.html), and [wake](https://hylic-recursion.github.io/hylic-docs/funnel/pool_dispatch.html). The [interactive benchmark viewer](https://hylic-recursion.github.io/hylic-docs/cookbook/benchmarks.html) walks the policy matrix. The [cookbook](https://hylic-recursion.github.io/hylic-docs/cookbook/fibonacci.html) has worked examples (filesystem summary, expression evaluation, module resolution, configuration inheritance, cycle detection, parallel execution).

## Sibling crates

[`hylic-pipeline`](https://github.com/hylic-recursion/hylic-pipeline) — typestate pipeline builder; re-exports `hylic::prelude::*` so it stands in for `hylic` in pipeline-using code.
[`hylic-benchmark`](https://github.com/hylic-recursion/hylic-benchmark) — Criterion harness behind the published numbers; not on crates.io.
[`hylic-docs`](https://github.com/hylic-recursion/hylic-docs) — mdBook source for the documentation site; not on crates.io.

## License

Licensed under the [MIT License](./LICENSE).
