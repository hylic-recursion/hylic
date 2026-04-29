# hylic

A Rust library for tree-shaped recursive computation. The
recursion is split into three values: a `Fold` (what to compute
at each node), a `Treeish` (how a node yields its children),
and an `Executor` (how the recursion runs). You build them
separately and hand them to the executor.

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

Folds and graphs are independently transformable. The same
fold runs unchanged over a flat adjacency list; only the graph
changes.

Documentation, an executor design deep-dive, an interactive
benchmark viewer, and a worked-example cookbook:
<https://hylic-recursion.github.io/hylic-docs/>.

A sibling crate,
[`hylic-pipeline`](https://github.com/hylic-recursion/hylic-pipeline),
adds a chainable typestate builder. Use it when you want
`.wrap_init(...).zipmap(...)` ergonomics; depend on `hylic`
alone otherwise.

## License

Licensed under the [MIT License](./LICENSE).
