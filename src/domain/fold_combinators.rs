//! Fold phase-wrapping combinators — small closure-level helpers
//! used by each domain's `wrap_*` sugar methods.

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
