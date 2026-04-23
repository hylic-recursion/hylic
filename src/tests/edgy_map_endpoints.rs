//! Edgy::map_endpoints — the sole slot-level primitive.

use std::sync::Arc;
use crate::graph::{edgy_visit, Edgy};

fn basic_edgy() -> Edgy<u64, u64> {
    // 0 → {1, 2}; 1 → {3}; others empty.
    edgy_visit(|n: &u64, cb: &mut dyn FnMut(&u64)| {
        match *n {
            0 => { cb(&1); cb(&2); }
            1 => { cb(&3); }
            _ => {}
        }
    })
}

#[test]
fn map_endpoints_identity() {
    let e = basic_edgy();
    let id: Edgy<u64, u64> = e.map_endpoints::<u64, u64, _>(|inner| inner);
    assert_eq!(id.apply(&0), vec![1, 2]);
    assert_eq!(id.apply(&1), vec![3]);
}

#[test]
fn map_endpoints_transforms_edges() {
    let doubled: Edgy<u64, u64> = basic_edgy().map_endpoints(|inner| {
        Arc::new(move |n: &u64, cb: &mut dyn FnMut(&u64)| {
            inner(n, &mut |e: &u64| cb(&(e * 2)))
        })
    });
    assert_eq!(doubled.apply(&0), vec![2, 4]);
}

#[test]
fn map_endpoints_transforms_nodes() {
    // Wrap N: u64 → String (via parse at the boundary).
    let stringed: Edgy<String, u64> = basic_edgy().map_endpoints(|inner| {
        Arc::new(move |s: &String, cb: &mut dyn FnMut(&u64)| {
            inner(&s.parse::<u64>().unwrap(), cb)
        })
    });
    assert_eq!(stringed.apply(&"0".to_string()), vec![1, 2]);
}

#[test]
fn sugars_delegate_to_map_endpoints() {
    // Each sugar matches its map_endpoints equivalent behaviourally.
    let e = basic_edgy();

    // map: E → String
    let mapped = e.map(|x: &u64| format!("{x}"));
    assert_eq!(mapped.apply(&0), vec!["1".to_string(), "2".to_string()]);

    // contramap: newN → u64
    let contra = e.contramap(|s: &String| s.parse::<u64>().unwrap());
    assert_eq!(contra.apply(&"0".to_string()), vec![1, 2]);

    // filter
    let filtered = basic_edgy().filter(|x: &u64| *x != 2);
    assert_eq!(filtered.apply(&0), vec![1]);
}
