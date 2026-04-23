//! Treeish-side Shared sugars — one-line wrappers over
//! `Shared::treeish_lift`. N, H, R preserved.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use crate::domain::Shared;
use crate::graph::{edgy_visit, Edgy};
use crate::ops::lift::shape::ShapeLift;

impl Shared {
    /// Filter the treeish's visible children by a predicate on N.
    pub fn filter_edges_lift<N, H, R, P>(pred: P)
        -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        P: Fn(&N) -> bool + Send + Sync + 'static,
    {
        let pred = Arc::new(pred);
        Shared::treeish_lift::<N, H, R, _>(move |g: Edgy<N, N>| {
            let p = pred.clone();
            g.filter(move |c: &N| p(c))
        })
    }

    /// Wrap the treeish's visit closure — user sees the node + the
    /// callback + the original visit closure. Use for debugging or
    /// conditional transformation during traversal.
    pub fn wrap_visit_lift<N, H, R, W>(wrapper: W)
        -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&N, &mut dyn FnMut(&N), &dyn Fn(&N, &mut dyn FnMut(&N)))
            + Send + Sync + 'static,
    {
        let w = Arc::new(wrapper);
        Shared::treeish_lift::<N, H, R, _>(move |g: Edgy<N, N>| {
            let w = w.clone();
            let g = g.clone();
            edgy_visit(move |n: &N, cb: &mut dyn FnMut(&N)| {
                let g_for_orig = g.clone();
                let orig = move |nn: &N, cbb: &mut dyn FnMut(&N)| g_for_orig.visit(nn, cbb);
                w(n, cb, &orig)
            })
        })
    }

    /// Memoize children by a user-supplied key function. Duplicate
    /// subtrees (identified by the same key) compute their children
    /// once; subsequent visits return the cached Vec.
    ///
    /// The cache mutex is scoped tightly: children are collected (or
    /// looked up) under the lock, the lock is dropped, and the
    /// callback is invoked on the released vector. This avoids
    /// reentrant acquisition under sequential executors whose
    /// `visit`-continuation recurses back through the same
    /// memoized graph before the outer call returns.
    pub fn memoize_by_lift<N, H, R, K, KeyFn>(key_fn: KeyFn)
        -> ShapeLift<Shared, N, H, R, N, H, R>
    where
        N: Clone + Send + Sync + 'static, H: Clone + 'static, R: Clone + 'static,
        K: Eq + Hash + Send + Sync + 'static,
        KeyFn: Fn(&N) -> K + Send + Sync + 'static,
    {
        let key_fn = Arc::new(key_fn);
        Shared::treeish_lift::<N, H, R, _>(move |g: Edgy<N, N>| {
            let key_fn = key_fn.clone();
            let cache: Arc<Mutex<HashMap<K, Vec<N>>>> = Arc::new(Mutex::new(HashMap::new()));
            edgy_visit(move |n: &N, cb: &mut dyn FnMut(&N)| {
                let k = key_fn(n);
                let children: Vec<N> = {
                    let mut cache_g = cache.lock().unwrap();
                    if let Some(cached) = cache_g.get(&k) {
                        cached.clone()
                    } else {
                        let mut collected: Vec<N> = Vec::new();
                        g.visit(n, &mut |c: &N| collected.push(c.clone()));
                        cache_g.insert(k, collected.clone());
                        collected
                    }
                };
                for c in &children { cb(c); }
            })
        })
    }
}
