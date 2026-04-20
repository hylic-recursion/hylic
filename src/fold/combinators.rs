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
