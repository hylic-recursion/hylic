# hylic

Decomposed recursive tree computation. Separates what to compute
(**Fold**) from the tree structure (**Treeish**) and how to execute
(**Exec**). Each piece is independently definable, transformable,
and composable.

## Quick example

```rust
use hylic::fold::simple_fold;
use hylic::graph::treeish;
use hylic::cata::Exec;

struct Dir { name: String, size: u64, children: Vec<Dir> }

let graph = treeish(|d: &Dir| d.children.clone());

let total_size = simple_fold(
    |d: &Dir| d.size,
    |heap: &mut u64, child: &u64| *heap += child,
);

let result = Exec::fused().run(&total_size, &graph, &root);
```

## Fold transformations

Folds are data — transform them without rewriting:

```rust
// Wrap init to add logging
let logged = total_size.map_init(|orig| Box::new(move |d: &Dir| {
    println!("visiting {}", d.name);
    orig(d)
}));

// Two folds in one pass
let both = total_size.product(&depth_fold());
let (size, depth) = Exec::fused().run(&both, &graph, &root);
```

## Parallel execution

Same fold, different executor:

```rust
// Fused: callback-based, zero allocation
let r1 = Exec::fused().run(&fold, &graph, &root);

// Rayon: parallel children, same result
let r2 = Exec::rayon().run(&fold, &graph, &root);
```

## Structure

| Module | Purpose |
|--------|---------|
| `graph` | Tree structure: `Edgy`, `Treeish`, `Graph` |
| `fold` | Fold algebra: `Fold`, `simple_fold`, type aliases |
| `cata` | Execution: `Exec` (fused, sequential, rayon), `Lift` (type-domain transformation) |
| `pipeline` | `GraphWithFold` — graph + fold + top-level entry = runnable pipeline |
| `prelude` | `VecFold`, `Explainer`, `memoize`, `seeds_for_fallible`, `uio_parallel` |
| `uio` | Lazy memoized computation (`UIO<T>`, `FnOnce`-based) |

`graph` includes `SeedGraph` for seed-based graph construction.
Core modules (`graph`, `fold`, `cata`) have no knowledge of higher
layers. `pipeline` wires graph + fold into runnable pipelines.
`prelude` provides batteries built on core.
