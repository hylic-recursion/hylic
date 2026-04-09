//! Combinator transforms — closure-level graph transformations.
//!
//! Each function takes a visit closure + transformation, returns a new
//! visit closure. Domain-independent: the returned closure inherits
//! Send+Sync from its captures via Rust's auto-trait system.
//!
//! Used by Shared (Edgy) and Local (Treeish) — each domain's combinator
//! method clones its storage, extracts the closure, passes it here,
//! re-wraps the result in domain-specific storage.

use either::Either;

/// Map edges: (N, E) → (N, NewE).
pub fn map_edges<N: 'static, E: 'static, NewE: 'static>(
    inner: impl Fn(&N, &mut dyn FnMut(&E)) + 'static,
    transform: impl Fn(&E) -> NewE + 'static,
) -> impl Fn(&N, &mut dyn FnMut(&NewE)) + 'static {
    move |n: &N, cb: &mut dyn FnMut(&NewE)| {
        inner(n, &mut |e: &E| {
            let mapped = transform(e);
            cb(&mapped);
        });
    }
}

/// Change node type: (N, E) → (NewN, E).
pub fn contramap_node<N: 'static, E: 'static, NewN: 'static>(
    inner: impl Fn(&N, &mut dyn FnMut(&E)) + 'static,
    transform: impl Fn(&NewN) -> N + 'static,
) -> impl Fn(&NewN, &mut dyn FnMut(&E)) + 'static {
    move |n: &NewN, cb: &mut dyn FnMut(&E)| {
        inner(&transform(n), cb);
    }
}

/// Change node type with fallback: either convert or provide edges.
pub fn contramap_or_node<N: 'static, E: 'static, NewN: 'static>(
    inner: impl Fn(&N, &mut dyn FnMut(&E)) + 'static,
    transform: impl Fn(&NewN) -> Either<N, Vec<E>> + 'static,
) -> impl Fn(&NewN, &mut dyn FnMut(&E)) + 'static {
    move |n: &NewN, cb: &mut dyn FnMut(&E)| {
        match transform(n) {
            Either::Left(node) => inner(&node, cb),
            Either::Right(edges) => { for e in &edges { cb(e); } }
        }
    }
}

/// Filter edges by predicate.
pub fn filter_edges<N: 'static, E: 'static>(
    inner: impl Fn(&N, &mut dyn FnMut(&E)) + 'static,
    pred: impl Fn(&E) -> bool + 'static,
) -> impl Fn(&N, &mut dyn FnMut(&E)) + 'static {
    move |n: &N, cb: &mut dyn FnMut(&E)| {
        inner(n, &mut |e: &E| { if pred(e) { cb(e); } });
    }
}

/// Both map + contramap for Treeish (E=N): (N, N) → (NewN, NewN).
pub fn treemap<N: 'static, NewN: 'static>(
    inner: impl Fn(&N, &mut dyn FnMut(&N)) + 'static,
    co_tf: impl Fn(&N) -> NewN + 'static,
    contra_tf: impl Fn(&NewN) -> N + 'static,
) -> impl Fn(&NewN, &mut dyn FnMut(&NewN)) + 'static {
    move |n: &NewN, cb: &mut dyn FnMut(&NewN)| {
        inner(&contra_tf(n), &mut |e: &N| {
            let mapped = co_tf(e);
            cb(&mapped);
        });
    }
}
