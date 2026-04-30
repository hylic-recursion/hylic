# hylic

A Rust library for tree-shaped recursive computation. Where you'd otherwise reach for `fn rec(node) -> R { … }` — hand-write the recursion now, write a parallel version of it later, and write another variant when the node type changes shape — hylic asks for the three pieces generically and runs the recursion for you. The same logic, defined once, runs sequentially or in parallel by switching the executor; over a struct tree or a flat adjacency list by switching the graph; with derived result types, edge filters, or pruned subtrees by composing transforms — without rewriting the closures.

The three pieces:

- A **graph** says how a node yields its children — `Treeish<N>`, morally `&N → [N]`. Tree shape lives in the graph function, not the data: a struct tree, a flat adjacency list, or an external lookup all build the same `Treeish<N>`. Trees and DAGs both work; cycles need to be broken in the node type.
- A **fold** says what to compute at each node — `Fold<N, H, R>`, three closures: `init: &N → H` builds a per-node heap, `accumulate: &mut H, &R` folds each child's result into the heap, `finalize: &H → R` produces the node's result.
- An **executor** drives the recursion — `FUSED` for direct sequential calls, or `Funnel` for parallel work-stealing.

```rust
use hylic::prelude::*;

#[derive(Clone)]
struct Dir { size: u64, children: Vec<Dir> }

let fold  = fold(|d: &Dir| d.size,
                 |heap: &mut u64, child: &u64| *heap += child,
                 |h: &u64| *h);
let graph = treeish(|d: &Dir| d.children.clone());

let total = FUSED.run(&fold, &graph, &dir);                          // sequential
let total = exec(funnel::Spec::default(4)).run(&fold, &graph, &dir); // parallel — same fold, same graph
```

The fold doesn't know what the graph looks like; the graph doesn't know what the fold computes; the executor connects them. That separation is the whole point. Switching to a flat adjacency list is a different `Treeish<usize>`. Switching to parallel is a different `Executor`. Deriving a new result type, dropping edges that don't matter, or caching shared subtrees is one of `map`, `contramap`, `filter`, `memoize` over the existing fold or graph — the original closures pass through unchanged.

Parallelism comes in the same package. `Funnel` is a work-stealing engine with three compile-time policy axes — queue topology, accumulation strategy, wake — that all monomorphise; there is no runtime dispatch on strategy choice. On the published 14-workload [Matrix benchmark](https://hylic-recursion.github.io/hylic-docs/cookbook/benchmarks.html#matrix), a `Funnel` variant wins ten rows outright against handrolled Rayon and a scoped pool, and lands within a few percent of the winner on the rest. `Funnel` ships in this crate; no extra dependency.

## Where to start

The book at <https://hylic-recursion.github.io/hylic-docs> is the long-form orientation: a [quick start](https://hylic-recursion.github.io/hylic-docs/quickstart.html), the underlying decomposition explained in [the recursive pattern](https://hylic-recursion.github.io/hylic-docs/concepts/separation.html), the [Funnel deep-dive](https://hylic-recursion.github.io/hylic-docs/funnel/overview.html) with chapters on [queue topology](https://hylic-recursion.github.io/hylic-docs/funnel/queue_strategies.html), [accumulation](https://hylic-recursion.github.io/hylic-docs/funnel/accumulation.html), and [wake](https://hylic-recursion.github.io/hylic-docs/funnel/pool_dispatch.html), the [interactive benchmark viewer](https://hylic-recursion.github.io/hylic-docs/cookbook/benchmarks.html), and a [cookbook](https://hylic-recursion.github.io/hylic-docs/cookbook/fibonacci.html) of worked examples (filesystem summary, expression evaluation, module resolution, configuration inheritance, cycle detection, parallel execution).

## Related crates

[`hylic-pipeline`](https://github.com/hylic-recursion/hylic-pipeline) layers a chainable typestate builder on top of these primitives — `.wrap_init(...).zipmap(...).run(...)` instead of values composed by hand. Optional; `hylic` alone is the core. [`hylic-benchmark`](https://github.com/hylic-recursion/hylic-benchmark) is the Criterion harness behind the published numbers; [`hylic-docs`](https://github.com/hylic-recursion/hylic-docs) is the mdBook source for the documentation site. Neither is on crates.io.

## License

Licensed under the [MIT License](./LICENSE).
