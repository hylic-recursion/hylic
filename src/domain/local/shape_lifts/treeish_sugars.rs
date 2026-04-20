//! Treeish-side Local sugars — one-line wrappers over
//! `Local::treeish_lift`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use crate::domain::Local;
use crate::domain::local::edgy::{edgy_visit, Edgy};
use crate::ops::lift::shape::universal::ShapeLift;

impl Local {
    pub fn filter_edges_lift<N, H, R, P>(pred: P) -> ShapeLift<Local, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        P: Fn(&N) -> bool + 'static,
    {
        let pred = Rc::new(pred);
        Local::treeish_lift::<N, H, R, _>(move |g: Edgy<N, N>| {
            let p = pred.clone();
            g.filter(move |c: &N| p(c))
        })
    }

    pub fn wrap_visit_lift<N, H, R, W>(wrapper: W) -> ShapeLift<Local, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        W: Fn(&N, &mut dyn FnMut(&N), &dyn Fn(&N, &mut dyn FnMut(&N))) + 'static,
    {
        let w = Rc::new(wrapper);
        Local::treeish_lift::<N, H, R, _>(move |g: Edgy<N, N>| {
            let w = w.clone();
            let g = g.clone();
            edgy_visit(move |n: &N, cb: &mut dyn FnMut(&N)| {
                let g_for_orig = g.clone();
                let orig = move |nn: &N, cbb: &mut dyn FnMut(&N)| g_for_orig.visit(nn, cbb);
                w(n, cb, &orig)
            })
        })
    }

    pub fn memoize_by_lift<N, H, R, K, KeyFn>(key_fn: KeyFn)
        -> ShapeLift<Local, N, H, R, N, H, R>
    where
        N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
        K: Eq + Hash + 'static,
        KeyFn: Fn(&N) -> K + 'static,
    {
        let key_fn = Rc::new(key_fn);
        Local::treeish_lift::<N, H, R, _>(move |g: Edgy<N, N>| {
            let key_fn = key_fn.clone();
            let cache: Rc<RefCell<HashMap<K, Vec<N>>>> = Rc::new(RefCell::new(HashMap::new()));
            edgy_visit(move |n: &N, cb: &mut dyn FnMut(&N)| {
                let k = key_fn(n);
                let mut cache_g = cache.borrow_mut();
                if let Some(children) = cache_g.get(&k) {
                    for c in children { cb(c); }
                    return;
                }
                let mut collected: Vec<N> = Vec::new();
                g.visit(n, &mut |c: &N| collected.push(c.clone()));
                for c in &collected { cb(c); }
                cache_g.insert(k, collected);
            })
        })
    }
}
