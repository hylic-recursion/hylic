//! Core-layer tests — independent of pipeline machinery.
//!
//! These tests exercise the Domain / Fold / Edgy / Lift primitives
//! directly, without going through pipeline typestates. Pipeline-
//! oriented tests live in `cata/pipeline/tests/`.

mod fold_domain_claims;
mod domain_constructors;
mod edgy_map_endpoints;
mod fold_map_phases;
