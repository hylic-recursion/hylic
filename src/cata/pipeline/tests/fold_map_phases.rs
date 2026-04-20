//! Fold::map_phases — the sole slot-level primitive.

use std::sync::Arc;
use crate::domain::shared::fold::fold;

#[test]
fn map_phases_identity() {
    let f = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let id: crate::domain::shared::fold::Fold<u64, u64, u64> =
        f.map_phases::<u64, u64, u64, _, _, _>(|i| i, |a| a, |fi| fi);

    // Behaviour preserved.
    let mut h = id.init(&5);
    assert_eq!(h, 5);
    id.accumulate(&mut h, &3);
    assert_eq!(h, 8);
    assert_eq!(id.finalize(&h), 8);
}

#[test]
fn map_phases_changes_r() {
    // Wrap R = u64 into String via map_phases.
    let f = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let s: crate::domain::shared::fold::Fold<u64, u64, String> = f.map_phases(
        |i| i,
        // acc with R=String needs to round-trip through u64.
        |a| Arc::new(move |h: &mut u64, s: &String| a(h, &s.parse::<u64>().unwrap())),
        |fi| Arc::new(move |h: &u64| format!("{}", fi(h))),
    );

    let mut h = s.init(&2);
    s.accumulate(&mut h, &"3".to_string());
    assert_eq!(s.finalize(&h), "5");
}

#[test]
fn sugars_delegate_to_map_phases() {
    // Each sugar should behave as if implemented via map_phases directly.
    let base = fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);

    // wrap_init(+10) via sugar
    let wrapped = base.wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 10);
    assert_eq!(wrapped.init(&3), 13);

    // zipmap via sugar
    let z = base.zipmap(|r: &u64| r * 2);
    let mut h = z.init(&5);
    z.accumulate(&mut h, &(1, 2));
    // heap = 5 + 1 = 6; finalize → (6, 12)
    assert_eq!(z.finalize(&h), (6, 12));
}
