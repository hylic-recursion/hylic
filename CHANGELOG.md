# Changelog

All notable changes to `hylic` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — Seed pipeline unification

### Changed (breaking)

- **`LiftedNode<N>` renamed to `SeedNode<N>`**, variant `Entry`
  renamed to `EntryRoot`. The Seed-rooted chain's lifted-node sum
  type is the new "first-class entry value" name. Backward-compatible
  aliases (`LiftedNode`, `Entry`) are kept for one cycle with
  `#[deprecated]`.
- **`SeedLift` is now domain-parametric over `ShapeCapable`**, with
  `entry_heap_fn: <D as ShapeCapable<N>>::EntryHeap<H>` (a per-domain
  GAT for `Fn() -> H` storage — `Arc` on Shared, `Rc` on Local) and
  `entry_seeds: <D as Domain<()>>::Graph<Seed>` (per-domain seed
  graph). The `EntryHeapFn<D, H>` enum and its `unreachable!` arms
  are gone. The Local-domain Send+Sync crack on `Seed` closes: a
  Local seeded run no longer requires `Seed: Send + Sync`.
- **`SeedExplainerResult::from_lifted` → `From` impl.** Project a
  raw seed-closed explainer trace via `raw.into()` (or
  `SeedExplainerResult::from(raw)`) — the standalone
  `from_lifted` associated function is removed.
- **`ConstructFold` trait removed** along with its unsafe
  `make_fold_unchecked`. ParLazy and ParEager migrate to safe
  `Domain::make_fold`; the closures' Send+Sync auto-derive from
  the wrapped `Fold<N, H, R>`.

### Added

- **Stage-2 unification**: `Stage2Pipeline<Base, L>` is the single
  Stage-2 pipeline type. Both treeish-rooted and seed-rooted chains
  flow through it, distinguished only by their `Base`. The historical
  `LiftedPipeline` and `LiftedSeedPipeline` are deprecated aliases.
- **`Wrap` trait + `Stage2Base` trait** as the dispatch backbone for
  Stage-2 sugars (the source-of-truth for "what does the sugar's
  `&N` parameter type unify with on this Base?").
- **`ShapeCapable::EntryHeap<H>` GAT** — domain-parametric storage for
  the `Fn() -> H` thunk consumed by `SeedLift` at EntryRoot init.

## [1.0.0] — 2026-05 (pending release)

The first stable release. This entry condenses the 0.x evolution
into the ten load-bearing changes that shaped the 1.0 surface.

### Added

- **Crate split into `hylic` (core) and `hylic-pipeline`
  (typestate builder)**, with the pipeline subtree relocated and
  the core left as `hylic::{ops, domain, graph, exec}`
  (`dd7f012`, `75e6428`; pipeline-side `f55fd44`).
- **`Lift` trait as a three-axis (Grow, Graph, Fold) transformer**
  with four building blocks: `IdentityLift`, `ComposedLift`,
  `ShapeLift`, `SeedLift`. The unified `ShapeLift` replaced a
  family of per-axis lift types (`516b859`, `36e870d`,
  `10719c8`).
- **`LiftBare` blanket trait** — apply any `Lift` to a bare
  `(treeish, fold)` pair without a pipeline (`516b859`).
- **`SeedLift` as a library `Lift`** — `PipelineExec::run`
  composes it onto the chain rather than inlining seed handling
  (`d21f305`).
- **Explainer lift family**: `explainer_lift`,
  `explainer_describe_lift`; and `treeish_for_explres` for
  downstream inspection of the trace tree (`dc9f915`, `b0d1d3f`).

### Changed

- **Naming regularised** to FP conventions: `_bi` suffix on every
  bijection-requiring method (`map_r_bi`, `map_n_bi_lift`, …);
  `_lift` suffix on library-lift constructors; per-axis prefixes
  where relevant (`59d4207`).
- **Layout flattened.** `cata/` removed; its contents live in
  `exec/`. `fold/` removed; `fold/combinators.rs` moved to
  `domain/fold_combinators.rs`. Prelude consolidated
  (`b803419`, `10eba03`, `9247121`).
- **Pipeline typestates made parametric over `D: Domain<N>`** —
  the same code compiles for `Shared`, `Local`, and `Owned`
  (`ab4c6ff`).
- **Blanket sugar traits per domain** (`SeedSugarsShared`,
  `TreeishSugarsShared`, `LiftedSugarsShared`, plus `_Local`
  mirrors) replacing `_local`-suffixed duplicate inherent
  methods (`2639c5d`, `80f918f`; pipeline-side `b8a4397`).

### Removed

- **`LiftedNode::Seed` and `LiftedHeap::Relay`** — unreachable
  after the SeedLift refactor (`099c311`).

### Notes

Three areas of code duplication (`domain/{shared,local}/shape_lifts/`
mirror files, `hylic-pipeline/src/sugars/{*_shared, *_local}.rs`
mirror files, the triplet `Fold` struct across
`domain/{shared,local,owned}`) were reviewed and explicitly
accepted as documented debt in
`hylic/KB/.plans/finishing-up/post-split-review/ACCEPTED-DEBT.md`
(`e30675e`, `f20783e`). Unification would require
`macro_rules!`, which the codebase declines; a trait-based
approach cannot express the domain-conditional closure bounds
(`Send + Sync` for Shared vs. unbounded for Local).
