//! Smoke test: can a user do useful work with just
//! `use hylic::prelude::*`?
//!
//! Run: `cargo run --example prelude_minimal`

use hylic::prelude::*;

fn main() {
    // (1) Build a Shared fold from the re-exported `fold` ctor.
    let f = fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    // (2) Build a Shared treeish via the re-exported `treeish` ctor.
    let t = treeish(|n: &u64| match *n {
        0 => vec![1, 2],
        1 => vec![3],
        _ => vec![],
    });

    // (3) Build a pipeline and chain sugar methods — all names come
    //     from prelude::*.
    let p: TreeishPipeline<Shared, u64, u64, u64> =
        TreeishPipeline::new(t, &f);

    let r: (u64, bool) = p
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .zipmap(|r: &u64| *r > 5)
        .run_from_node(&exec(funnel::Spec::default(4)), &0u64);

    // 0:1, 1:2+4=6, 2:3, 3:4. Tree sum = 1+6+3 = 10. wrap_init adds +1
    // to each init, plus root ceremony. Details covered by test suite.
    println!("(prelude minimal) r = {r:?}");
    assert_eq!(r.0, 10);
    assert_eq!(r.1, true);

    // (4) Shared FUSED executor also available via prelude.
    let r2: u64 = TreeishPipeline::<Shared, u64, u64, u64>::new(
        treeish(|n: &u64| if *n == 0 { vec![1] } else { vec![] }),
        &fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h),
    )
    .run_from_node(&FUSED, &0u64);
    assert_eq!(r2, 1);

    println!("All prelude-minimal checks OK.");
}
