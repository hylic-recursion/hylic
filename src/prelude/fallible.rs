//! Helpers for the fallible seed pattern: nodes are Either<Error, Valid>,
//! where errors are leaves (no seeds) and valid nodes produce seeds.
//!
//! This is a prelude convenience, not a core concept. The core SeedGraph
//! is generic over any Node type.

use either::Either;
use crate::graph::Edgy;

/// Lift an Edgy<ValidNode, Seed> to Edgy<Either<Err, ValidNode>, Seed>.
/// Valid nodes delegate to the inner edgy; error nodes produce no seeds.
pub fn seeds_for_fallible<V, E, S>(
    seeds_from_valid: Edgy<V, S>,
) -> Edgy<Either<E, V>, S>
where V: Clone + 'static, E: 'static, S: 'static,
{
    seeds_from_valid.contramap_or(|node: &Either<E, V>| match node {
        Either::Right(valid) => Either::Left(valid.clone()),
        Either::Left(_) => Either::Right(vec![]),
    })
}
