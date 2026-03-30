# hylic

Composable recursive tree computation for Rust.

Define a fold (what to compute at each node), a graph (how to traverse
children), and run them together. Logging, caching, error handling, and
parallelism are natural transformations on these pieces — not rewrites
of the recursion itself.

## The problem

Most recursive algorithms follow a two-function pattern: an entry point
that sets up the first node, and a recursive function that processes
children. They share fold logic but differ in how they enter the
recursion. As the algorithm grows, concerns tangle — error handling,
logging, and caching get woven into both functions.

hylic separates the three concerns:

- **Fold** — what to compute: initialize a heap per node, accumulate
  child results, finalize into the node's result.
- **Graph** — the tree structure: given a node, visit its children.
- **Strategy** — how to execute: sequential, parallel traversal,
  or lazy parallel fold.

Each is defined once, independently transformable, and composable.

## Quick example

```rust
use hylic::fold::simple_fold;
use hylic::graph::treeish;
use hylic::cata::Strategy;

#[derive(Clone)]
struct Dir { name: String, size: u64, children: Vec<Dir> }

let graph = treeish(|d: &Dir| d.children.clone());

let total_size = simple_fold(
    |d: &Dir| d.size,
    |heap: &mut u64, child: &u64| *heap += child,
);

let result = Strategy::Sequential.run(&total_size, &graph, &root);
```

## Transformations

The fold and graph are data — you transform them, not rewrite them:

```rust
// Add logging to initialization
let logged = my_fold.map_init(|original_init| {
    Box::new(move |node| {
        eprintln!("visiting {:?}", node);
        original_init(node)
    })
});

// Change what the fold produces
let with_count = my_fold.zipmap(|result| 1usize); // (R, usize) — result + count
```

## Parallel execution

Same fold, different strategy:

```rust
use hylic::cata::{Strategy, ALL};

let result = Strategy::ParTraverse.run(&fold, &graph, &root);

// Verify all strategies produce the same result
for s in ALL {
    assert_eq!(s.run(&fold, &graph, &root), expected);
}
```

## Structure

hylic is organized into layers with strict dependency flow:

| Module | Purpose |
|--------|---------|
| `graph` | Tree structure: `Edgy`, `Treeish`, `Graph` |
| `fold` | Fold algebra: `Fold`, `simple_fold`, `fold` |
| `cata` | Execution: `Strategy` (Sequential, ParTraverse, ParFoldLazy) |
| `ana` | Seed-based graph construction (anamorphism) |
| `hylo` | Fold+graph adapters for composed pipelines |
| `prelude` | Helpers: `VecFold`, `Explainer`, `TreeFormatCfg`, `Visit` |

Core modules (`graph`, `fold`, `cata`) have no knowledge of higher
layers. `ana` builds graphs from seeds. `hylo` wires fold and graph
together. `prelude` provides convenience types built on core.

## Background

The fold algebra corresponds to a monoidal tree catamorphism — a bottom-up
tree fold decomposed into initialize / accumulate / finalize phases.
The graph traversal externalizes the tree structure as a function rather
than encoding it in the type system. When a graph is lazily constructed
(via `ana`), the combined unfold+fold is a hylomorphism — hence the name.

The `Explainer` wraps any fold to record the full computation trace,
corresponding to a histomorphism in recursion scheme terminology.

For more on the formal background and how hylic relates to Haskell's
`recursion-schemes` library, see the [theory section](doc/) in the
documentation crate.

## Related projects

- [recursion](https://crates.io/crates/recursion) — Rust recursion
  schemes via fixed-point functors. More type-theoretically principled;
  hylic trades functor encoding for runtime flexibility (externalized
  tree structure, callback-based traversal).
- [recursion-schemes](https://hackage.haskell.org/package/recursion-schemes)
  (Haskell) — the canonical implementation by Edward Kmett. hylic's fold
  corresponds to `cata`, the explainer to `histo`, seed graphs to `ana`.
- [salsa](https://crates.io/crates/salsa) — demand-driven incremental
  computation. Different paradigm (query databases vs tree folds), but
  addresses similar problems around memoization and recomputation.

## License

MIT OR Apache-2.0
