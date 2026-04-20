//! TDD assertions for the Phase-5/1 claim: `FoldTransformsByRef` on
//! Local and `FoldTransformsByValue` on Owned accept non-Send
//! closures, while Shared still requires Send+Sync.
//!
//! These tests pin the per-domain bound differences. They are the
//! ground truth for the (a-uniform) claim "every closure position
//! other than grow-construction accepts non-Send captures on
//! Local/Owned."

use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn local_fold_wrap_init_accepts_non_send_capture() {
    use crate::domain::local;

    // Capture Rc<RefCell<…>> — definitively not Send+Sync.
    let counter: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));

    let base = local::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    // wrap_init's wrapper captures counter. Works on Local.
    let counter_for_wrap = counter.clone();
    let wrapped = base.wrap_init(move |n: &u64, orig: &dyn Fn(&u64) -> u64| {
        *counter_for_wrap.borrow_mut() += 1;
        orig(n)
    });

    // Drive it manually (no pipeline, no executor — just Fold-level check).
    let h0 = wrapped.init(&5);
    let h1 = wrapped.init(&10);
    assert_eq!(h0, 5);
    assert_eq!(h1, 10);
    assert_eq!(*counter.borrow(), 2, "wrapper fired twice");
}

#[test]
fn local_fold_contramap_accepts_non_send_capture() {
    use crate::domain::local;

    let tag: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let base = local::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let tag_for_contra = tag.clone();
    // contramap accepts a non-Send closure under Local.
    let contra = base.contramap_n(move |s: &String| {
        tag_for_contra.borrow_mut().push_str(s);
        s.len() as u64
    });

    let h = contra.init(&"hello".to_string());
    assert_eq!(h, 5);
    assert_eq!(*tag.borrow(), "hello");
}

#[test]
fn owned_fold_wrap_finalize_with_non_send_state_moves_in() {
    use crate::domain::owned;

    // Owned fold consumes self on wrap. The wrapper closure can
    // capture non-Send state freely.
    let trace: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));

    let base = owned::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let trace_for_wrap = trace.clone();
    let wrapped = base.wrap_finalize(move |h: &u64, orig: &dyn Fn(&u64) -> u64| {
        let r = orig(h);
        trace_for_wrap.borrow_mut().push(r);
        r
    });

    let h = wrapped.init(&7);
    let r = wrapped.finalize(&h);
    assert_eq!(r, 7);
    assert_eq!(trace.borrow().as_slice(), &[7u64]);
}

#[test]
fn owned_fold_contramap_consumes_and_accepts_non_send() {
    use crate::domain::owned;

    let shared_counter: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));

    let base = owned::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    // Owned's contramap consumes self.
    let counter_for_contra = shared_counter.clone();
    let contra = base.contramap_n(move |s: &String| {
        *counter_for_contra.borrow_mut() += 1;
        s.len() as u64
    });

    let h = contra.init(&"hi".to_string());
    assert_eq!(h, 2);
    assert_eq!(*shared_counter.borrow(), 1);
}

#[test]
fn local_fold_full_pipeline_non_send_captures() {
    // Chain multiple non-Send-capturing transformations on Local.
    use crate::domain::local;

    let visits: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));

    let base = local::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let v1 = visits.clone();
    let wrapped_init = base.wrap_init(move |n, orig| {
        v1.borrow_mut().push(*n);
        orig(n) + 10
    });

    let v2 = visits.clone();
    let wrapped_fin = wrapped_init.wrap_finalize(move |h: &u64, orig: &dyn Fn(&u64) -> u64| {
        let r = orig(h);
        v2.borrow_mut().push(r * 100);
        r
    });

    let h = wrapped_fin.init(&3);
    let r = wrapped_fin.finalize(&h);
    assert_eq!(h, 13);  // 3 + 10 via wrap_init
    assert_eq!(r, 13);  // identity finalize + side effect
    assert_eq!(visits.borrow().as_slice(), &[3u64, 1300]);
}

#[test]
fn local_fold_transforms_byref_trait_object_imported() {
    // Ensure the FoldTransformsByRef trait can be imported and the
    // bound on Fold<…> lets us call map_phases in generic code.
    use crate::domain::local;
    use crate::ops::FoldTransformsByRef;
    use std::rc::Rc;

    fn generic_wrap<F>(f: F) -> F
    where F: FoldTransformsByRef<u64, u64, u64>,
    {
        f
    }

    let base: local::Fold<u64, u64, u64> = local::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let _as_trait: local::Fold<u64, u64, u64> = generic_wrap(base);

    // Also verify map_phases directly.
    let base2: local::Fold<u64, u64, u64> = local::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let reshaped: local::Fold<u64, u64, u64> =
        <local::Fold<u64, u64, u64> as FoldTransformsByRef<u64, u64, u64>>::map_phases::<u64, u64, u64, _, _, _>(
            &base2,
            |i| Rc::new(move |n: &u64| i(n) * 2),
            |a| a,
            |f| f,
        );
    assert_eq!(reshaped.init(&5), 10);
}
