//! Phase 5/3 — Domain trait constructors (`make_fold`, `make_grow`,
//! `make_graph`) work uniformly across Shared, Local, Owned. Plus:
//! local::Edgy and owned::Edgy accept non-Send closures at
//! construction and transformation sites.

use std::cell::RefCell;
use std::rc::Rc;
use crate::domain::{Domain, Shared, Local, Owned};
use crate::domain::local::edgy as local_edgy;
use crate::domain::owned::edgy as owned_edgy;

#[test]
fn make_fold_shared() {
    let f: <Shared as Domain<u64>>::Fold<u64, u64> =
        <Shared as Domain<u64>>::make_fold(
            |n: &u64| *n,
            |h: &mut u64, c: &u64| *h += c,
            |h: &u64| *h,
        );
    let mut h = f.init(&3);
    f.accumulate(&mut h, &4);
    assert_eq!(f.finalize(&h), 7);
}

#[test]
fn make_fold_local() {
    let f: <Local as Domain<u64>>::Fold<u64, u64> =
        <Local as Domain<u64>>::make_fold(
            |n: &u64| *n,
            |h: &mut u64, c: &u64| *h += c,
            |h: &u64| *h,
        );
    let mut h = f.init(&3);
    f.accumulate(&mut h, &4);
    assert_eq!(f.finalize(&h), 7);
}

#[test]
fn make_fold_owned() {
    let f: <Owned as Domain<u64>>::Fold<u64, u64> =
        <Owned as Domain<u64>>::make_fold(
            |n: &u64| *n,
            |h: &mut u64, c: &u64| *h += c,
            |h: &u64| *h,
        );
    let mut h = f.init(&3);
    f.accumulate(&mut h, &4);
    assert_eq!(f.finalize(&h), 7);
}

#[test]
fn make_grow_invoke_all_domains() {
    let g_shared = <Shared as Domain<u64>>::make_grow::<String, u64>(|s: &String| s.len() as u64);
    assert_eq!(<Shared as Domain<u64>>::invoke_grow(&g_shared, &"hello".to_string()), 5);

    let g_local = <Local as Domain<u64>>::make_grow::<String, u64>(|s: &String| s.len() as u64);
    assert_eq!(<Local as Domain<u64>>::invoke_grow(&g_local, &"hi".to_string()), 2);

    let g_owned = <Owned as Domain<u64>>::make_grow::<String, u64>(|s: &String| s.len() as u64);
    assert_eq!(<Owned as Domain<u64>>::invoke_grow(&g_owned, &"x".to_string()), 1);
}

#[test]
fn make_graph_shared() {
    let g: <Shared as Domain<u64>>::Graph<u64> =
        <Shared as Domain<u64>>::make_graph(|n: &u64, cb: &mut dyn FnMut(&u64)| {
            if *n == 0 { cb(&1); cb(&2); }
        });
    let kids = g.apply(&0);
    assert_eq!(kids, vec![1, 2]);
}

#[test]
fn make_graph_local() {
    let g: <Local as Domain<u64>>::Graph<u64> =
        <Local as Domain<u64>>::make_graph(|n: &u64, cb: &mut dyn FnMut(&u64)| {
            if *n == 0 { cb(&1); cb(&2); }
        });
    let kids = g.apply(&0);
    assert_eq!(kids, vec![1, 2]);
}

#[test]
fn make_graph_owned() {
    let g: <Owned as Domain<u64>>::Graph<u64> =
        <Owned as Domain<u64>>::make_graph(|n: &u64, cb: &mut dyn FnMut(&u64)| {
            if *n == 0 { cb(&1); cb(&2); }
        });
    let kids = g.apply(&0);
    assert_eq!(kids, vec![1, 2]);
}

// ── Key claim tests: non-Send closures in per-domain Edgy ──

#[test]
fn local_edgy_filter_with_non_send_predicate() {
    // local::Edgy::filter accepts a non-Send predicate. Under the
    // Arc-based shared Edgy this would fail; the per-domain Edgy
    // relaxes the bound.
    let counter: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));

    let g = local_edgy::edgy(|n: &u64| {
        if *n == 0 { vec![1u64, 2, 3] } else { vec![] }
    });

    let counter_for_filter = counter.clone();
    let filtered = g.filter(move |e: &u64| {
        *counter_for_filter.borrow_mut() += 1;
        *e != 2
    });

    let kids = filtered.apply(&0);
    assert_eq!(kids, vec![1u64, 3]);
    assert_eq!(*counter.borrow(), 3);  // predicate fired 3 times
}

#[test]
fn local_edgy_map_with_non_send_capture() {
    let tag: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let g = local_edgy::edgy(|n: &u64| {
        if *n == 0 { vec![10u64, 20] } else { vec![] }
    });

    let tag_for_map = tag.clone();
    let mapped = g.map(move |e: &u64| {
        tag_for_map.borrow_mut().push_str(&format!("{e};"));
        e + 1000
    });

    let kids = mapped.apply(&0);
    assert_eq!(kids, vec![1010u64, 1020]);
    assert_eq!(*tag.borrow(), "10;20;");
}

#[test]
fn local_edgy_contramap_non_send() {
    let seen_inputs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    let g: local_edgy::Edgy<u64, u64> = local_edgy::edgy(|n: &u64| {
        if *n == 0 { vec![1u64] } else { vec![] }
    });

    let inputs_for_contra = seen_inputs.clone();
    let contra: local_edgy::Edgy<String, u64> = g.contramap(move |s: &String| {
        inputs_for_contra.borrow_mut().push(s.clone());
        s.parse::<u64>().unwrap()
    });

    let kids = contra.apply(&"0".to_string());
    assert_eq!(kids, vec![1u64]);
    assert_eq!(seen_inputs.borrow().as_slice(), &["0".to_string()]);
}

#[test]
fn owned_edgy_consuming_transforms() {
    let g: owned_edgy::Edgy<u64, u64> = owned_edgy::edgy(|n: &u64| {
        if *n == 0 { vec![1, 2, 3] } else { vec![] }
    });

    // Owned consumes on transform; filter → map is a chain.
    let counter: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));
    let counter_for_map = counter.clone();
    let transformed = g
        .filter(|e: &u64| *e != 2)
        .map(move |e: &u64| {
            *counter_for_map.borrow_mut() += 1;
            e * 10
        });

    let kids = transformed.apply(&0);
    assert_eq!(kids, vec![10u64, 30]);
    assert_eq!(*counter.borrow(), 2);
}

#[test]
fn owned_edgy_contramap_non_send_capture() {
    let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    let g: owned_edgy::Edgy<u64, u64> = owned_edgy::edgy(|n: &u64| {
        if *n == 0 { vec![42u64] } else { vec![] }
    });

    let log_for_contra = log.clone();
    let contra: owned_edgy::Edgy<String, u64> = g.contramap(move |s: &String| {
        log_for_contra.borrow_mut().push(s.clone());
        s.parse().unwrap()
    });

    let kids = contra.apply(&"0".to_string());
    assert_eq!(kids, vec![42u64]);
    assert_eq!(log.borrow().len(), 1);
}
