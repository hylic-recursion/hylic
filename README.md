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

Same fold, different approaches — identical results:

```rust
// Direct: rayon parallelizes child visiting
let r1 = Exec::rayon().run(&fold, &graph, &root);

// Lifted: lazy ParRef tree, eval triggers parallel bottom-up
let r2 = Exec::fused().run_lifted(&ParLazy::lift(), &fold, &graph, &root);

// Lifted: eager fork-join with a scoped WorkPool
ParEager::with(WorkPoolSpec::threads(3), |lift| {
    let r3 = Exec::fused().run_lifted(lift, &fold, &graph, &root);
});
```

## Lifts

`Lift` transforms a fold's type domain — enabling parallelism,
tracing, or any enrichment — without rewriting the fold. The
caller gets back the original result type transparently.

```rust
// Explainer: record computation trace as a Lift
let r = Exec::fused().run_lifted(&Explainer::lift(), &fold, &graph, &root);
```

All three built-in Lifts (`Explainer`, `ParLazy`, `ParEager`) follow
the same pattern: transform fold + treeish, run via `Exec::run_lifted`,
unwrap the result.

## Structure

| Module | Purpose |
|--------|---------|
| `graph` | Tree structure: `Edgy`, `Treeish`, `Graph`, `SeedGraph` |
| `fold` | Fold algebra: `Fold`, `simple_fold`, type aliases |
| `cata` | Execution: `Exec` (fused, sequential, rayon), `Lift` (type-domain transformation) |
| `parref` | `ParRef<T>` — lazy memoized computation (`FnOnce`-based) |
| `pipeline` | `GraphWithFold` — graph + fold + top-level entry = runnable pipeline |
| `prelude` | `VecFold`, `Explainer`, `memoize`, `seeds_for_fallible`, `ParLazy`, `ParEager`, `WorkPool` |

Core modules (`graph`, `fold`, `cata`) have no knowledge of higher
layers. `pipeline` wires graph + fold into runnable pipelines.
`prelude` provides batteries built on core.
