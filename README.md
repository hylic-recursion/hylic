# hylic

A Rust library that decomposes tree-shaped recursive computation into three independent parts:

- A **graph** that says how a node yields its children. Type: `Treeish<N>`, morally `N -> &[N]`.
- A **fold** that says what to compute at each node. Type: `Fold<N, H, R>`. Three closures: `init: &N -> H` builds a per-node heap, `accumulate: &mut H, &R` folds each child's result into the heap, `finalize: &H -> R` produces the node's result.
- An **executor** that drives the recursion. `FUSED` for sequential, `Funnel` for parallel work-stealing.

Instead of writing a recursive function by hand, you build these three values and hand them to the executor. The fold doesn't know the graph. The graph doesn't know the fold. The executor connects them.

```rust
use hylic::prelude::*;

#[derive(Clone)]
struct Dir { size: u64, children: Vec<Dir> }

let fold  = fold(|d: &Dir| d.size,
                 |heap: &mut u64, child: &u64| *heap += child,
                 |h: &u64| *h);
let graph = treeish(|d: &Dir| d.children.clone());

let total = FUSED.run(&fold, &graph, &dir);                          // sequential
let total = exec(funnel::Spec::default(4)).run(&fold, &graph, &dir); // parallel, same fold + graph
```

Express the logic once. The same fold runs sequentially or in parallel; only the executor swaps. It runs over a struct tree or a flat adjacency list; only the graph swaps. Need a different result type, edges filtered, subtrees pruned, shared subtrees memoized? Compose `map`, `contramap`, `filter`, `memoize` over the existing values. The original closures pass through unchanged.

Tree shape lives in the graph function, not the data. A struct tree, a flat adjacency list, or an external lookup all build the same `Treeish<N>`. Trees and DAGs both work. Cycles need to be broken in the node type.

## Parallel execution

`Funnel` is a work-stealing engine bundled in this crate; no extra dependency. Three compile-time policy axes (queue topology, accumulation strategy, wake) all monomorphise, so there is no runtime dispatch on strategy choice. On the published 14-workload [Matrix benchmark](https://hylic-recursion.github.io/hylic-docs/cookbook/benchmarks.html#matrix), a `Funnel` variant wins ten rows outright against handrolled Rayon and a scoped pool, and lands within a few percent of the winner on the rest.

## Where to start

The book at <https://hylic-recursion.github.io/hylic-docs> is the long-form orientation. The [quick start](https://hylic-recursion.github.io/hylic-docs/quickstart.html) is a complete fold in three closures. [The recursive pattern](https://hylic-recursion.github.io/hylic-docs/concepts/separation.html) explains what fold, graph, and executor each contribute, and why they stay separate. The [Funnel deep-dive](https://hylic-recursion.github.io/hylic-docs/funnel/overview.html) covers the parallel engine, with chapters on [queue topology](https://hylic-recursion.github.io/hylic-docs/funnel/queue_strategies.html), [accumulation](https://hylic-recursion.github.io/hylic-docs/funnel/accumulation.html), and [wake](https://hylic-recursion.github.io/hylic-docs/funnel/pool_dispatch.html). The [interactive benchmark viewer](https://hylic-recursion.github.io/hylic-docs/cookbook/benchmarks.html) walks the 16-policy matrix; the [cookbook](https://hylic-recursion.github.io/hylic-docs/cookbook/fibonacci.html) collects worked examples (filesystem summary, expression evaluation, module resolution, configuration inheritance, cycle detection, parallel execution).

## Related crates

[`hylic-pipeline`](https://github.com/hylic-recursion/hylic-pipeline) layers a chainable typestate builder on top of these primitives, so you can write `.wrap_init(...).zipmap(...).run(...)` instead of composing values by hand. Optional; `hylic` alone is the core. [`hylic-benchmark`](https://github.com/hylic-recursion/hylic-benchmark) is the Criterion harness behind the published numbers. [`hylic-docs`](https://github.com/hylic-recursion/hylic-docs) is the mdBook source for the documentation site. Neither is on crates.io.

## License

Licensed under the [MIT License](./LICENSE).
