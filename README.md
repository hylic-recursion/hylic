# hylic

A Rust library for computing things over trees: directory
traversal, dependency resolution, expression evaluation —
anywhere the pattern is *process each node, combine with the
results from its children*. hylic structures that pattern into
three independent values: a `Fold` (what to compute at each
node), a `Treeish` (how a node yields its children), and an
`Executor` (how the recursion runs).

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

Defining the fold once is enough. The same fold runs
sequentially or in parallel by swapping the executor; it runs
over a struct tree or a flat adjacency list by swapping the
treeish; it composes with transforms (`map`, `contramap`,
`filter`, `memoize`) without rewriting the closures.

## Reading order for newcomers

The book is the long-form orientation:

- **[Quick start](https://hylic-recursion.github.io/hylic-docs/quickstart.html)** — a complete fold in three closures.
- **[The recursive pattern](https://hylic-recursion.github.io/hylic-docs/concepts/separation.html)** — what fold / treeish / executor each contribute, and why they stay separate.
- **[The Funnel executor](https://hylic-recursion.github.io/hylic-docs/funnel/overview.html)** — work-stealing parallelism with three monomorphised policy axes ([queue topology](https://hylic-recursion.github.io/hylic-docs/funnel/queue_strategies.html), [accumulation](https://hylic-recursion.github.io/hylic-docs/funnel/accumulation.html), [wake](https://hylic-recursion.github.io/hylic-docs/funnel/pool_dispatch.html)).
- **[Benchmarks](https://hylic-recursion.github.io/hylic-docs/cookbook/benchmarks.html)** — interactive viewer over the 16-policy matrix; comparison against Rayon and handrolled baselines.
- **[Cookbook](https://hylic-recursion.github.io/hylic-docs/cookbook/fibonacci.html)** — worked examples ([filesystem summary](https://hylic-recursion.github.io/hylic-docs/cookbook/filesystem_summary.html), [expression evaluation](https://hylic-recursion.github.io/hylic-docs/cookbook/expression_eval.html), [module resolution](https://hylic-recursion.github.io/hylic-docs/cookbook/module_resolution.html), [configuration inheritance](https://hylic-recursion.github.io/hylic-docs/cookbook/config_inheritance.html), [cycle detection](https://hylic-recursion.github.io/hylic-docs/cookbook/cycle_detection.html)).

## Related crates

- [`hylic-pipeline`](https://github.com/hylic-recursion/hylic-pipeline)
  — chainable typestate builder over the same primitives. Use
  it when you'd rather call `.wrap_init(...).zipmap(...)` than
  compose values by hand. Optional; depend on `hylic` alone if
  the bare API is enough.
- [`hylic-benchmark`](https://github.com/hylic-recursion/hylic-benchmark)
  — Criterion harness behind the published benchmark results.
  Not on crates.io.
- [`hylic-docs`](https://github.com/hylic-recursion/hylic-docs)
  — mdBook source for the documentation site linked above.
  Not on crates.io.

## License

Licensed under the [MIT License](./LICENSE).
