//! Fold combinator transforms — closure-level logic, domain-independent.
//!
//! Each function takes fold closures + transformation, returns new closures.
//! Auto-traits propagate Send+Sync from inputs.

// ── Type-changing combinators ──────────────────────

/// map: change result type R → RNew.
pub fn map_fold<N: 'static, H: 'static, R: 'static, RNew: 'static>(
    init: impl Fn(&N) -> H + 'static,
    acc: impl Fn(&mut H, &R) + 'static,
    fin: impl Fn(&H) -> R + 'static,
    mapper: impl Fn(&R) -> RNew + 'static,
    backmapper: impl Fn(&RNew) -> R + 'static,
) -> (
    impl Fn(&N) -> H + 'static,
    impl Fn(&mut H, &RNew) + 'static,
    impl Fn(&H) -> RNew + 'static,
) {
    (
        move |n: &N| init(n),
        move |h: &mut H, r: &RNew| acc(h, &backmapper(r)),
        move |h: &H| mapper(&fin(h)),
    )
}

/// contramap: change node type N → NewN.
pub fn contramap_fold<N: 'static, H: 'static, R: 'static, NewN: 'static>(
    init: impl Fn(&N) -> H + 'static,
    acc: impl Fn(&mut H, &R) + 'static,
    fin: impl Fn(&H) -> R + 'static,
    transform: impl Fn(&NewN) -> N + 'static,
) -> (
    impl Fn(&NewN) -> H + 'static,
    impl Fn(&mut H, &R) + 'static,
    impl Fn(&H) -> R + 'static,
) {
    (
        move |new_n: &NewN| init(&transform(new_n)),
        move |h: &mut H, r: &R| acc(h, r),
        move |h: &H| fin(h),
    )
}

/// product: run two folds in one traversal.
pub fn product_fold<N: 'static, H1: 'static, R1: 'static, H2: 'static, R2: 'static>(
    init1: impl Fn(&N) -> H1 + 'static,
    acc1: impl Fn(&mut H1, &R1) + 'static,
    fin1: impl Fn(&H1) -> R1 + 'static,
    init2: impl Fn(&N) -> H2 + 'static,
    acc2: impl Fn(&mut H2, &R2) + 'static,
    fin2: impl Fn(&H2) -> R2 + 'static,
) -> (
    impl Fn(&N) -> (H1, H2) + 'static,
    impl Fn(&mut (H1, H2), &(R1, R2)) + 'static,
    impl Fn(&(H1, H2)) -> (R1, R2) + 'static,
) {
    (
        move |n: &N| (init1(n), init2(n)),
        move |heap: &mut (H1, H2), child: &(R1, R2)| {
            acc1(&mut heap.0, &child.0);
            acc2(&mut heap.1, &child.1);
        },
        move |heap: &(H1, H2)| (fin1(&heap.0), fin2(&heap.1)),
    )
}

// ── Phase-wrapping combinators ─────────────────────

/// Wrap init: intercept each init call.
pub fn wrap_init<N: 'static, H: 'static>(
    init: impl Fn(&N) -> H + 'static,
    wrapper: impl Fn(&N, &dyn Fn(&N) -> H) -> H + 'static,
) -> impl Fn(&N) -> H + 'static {
    move |n: &N| wrapper(n, &init)
}

/// Wrap accumulate: intercept each accumulate call.
pub fn wrap_accumulate<H: 'static, R: 'static>(
    acc: impl Fn(&mut H, &R) + 'static,
    wrapper: impl Fn(&mut H, &R, &dyn Fn(&mut H, &R)) + 'static,
) -> impl Fn(&mut H, &R) + 'static {
    move |h: &mut H, r: &R| wrapper(h, r, &acc)
}

/// Wrap finalize: intercept each finalize call.
pub fn wrap_finalize<H: 'static, R: 'static>(
    fin: impl Fn(&H) -> R + 'static,
    wrapper: impl Fn(&H, &dyn Fn(&H) -> R) -> R + 'static,
) -> impl Fn(&H) -> R + 'static {
    move |h: &H| wrapper(h, &fin)
}
