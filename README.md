# hylic

Decomposed recursive tree computation. Separates what to compute
(**Fold**) from the tree structure (**Treeish**) and how to execute
(**Exec**). Each piece is independently definable, transformable,
and composable.

Three boxing domains — `Shared` (Arc, parallel-friendly), `Local`
(Rc, single-threaded, non-`Send` captures), `Owned` (Box, one-shot)
— select how closures are stored. A `Lift<D, N, H, R>` is the
universal CPS transformer over `(Grow, Treeish, Fold)`.

Pipeline typestates and their chainable sugars live in the sibling
crate [`hylic-pipeline`](../hylic-pipeline/). `hylic` alone gives
you the lift primitives + bare execution (`LiftBare::run_on`).

## Documentation

See the `hylic-docs` book (the `docs-build` / `docs-serve` targets
in the workspace root `Makefile`). That book is the authoritative
usage reference; this README intentionally stays terse.

Quick shortcuts (from the workspace root):

```
make test              # every workspace crate's lib tests (306)
make docs-serve        # build and serve the docs book locally
make bench-integration # opt-in ParLazy / ParEager benchmark matrix
```

## Prelude

```rust
use hylic::prelude::*;
```

brings: `Shared` + Fold/Edgy/Treeish constructors, `Exec` + `fused`
/ `funnel` module aliases, every Lift atom (`Lift`, `IdentityLift`,
`ComposedLift`, `ShapeLift`, `SeedLift`, `LiftBare`, capability
markers, `LiftedNode`), and common debug helpers (explainer,
pretty-printers, vec-fold).

For pipeline typestates add:

```rust
use hylic_pipeline::prelude::*;   // re-exports hylic::prelude::*
                                  // plus SeedPipeline, LiftedSugarsShared, etc.
```
