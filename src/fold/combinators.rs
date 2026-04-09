//! Fold combinator transforms — closure-level logic, domain-independent.
//!
//! Each function takes the three fold closures (init, accumulate, finalize)
//! plus a transformation, and returns three new closures. Auto-traits
//! propagate Send+Sync from inputs.

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
        move |node: &N| init(node),
        move |heap: &mut H, result: &RNew| { acc(heap, &backmapper(result)); },
        move |heap: &H| mapper(&fin(heap)),
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
