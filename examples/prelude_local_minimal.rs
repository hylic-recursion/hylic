//! Smoke test: user-facing Local-domain imports.
//! Run: `cargo run --example prelude_local_minimal`

use hylic::prelude::*;
use hylic::prelude::local::{Local, LiftedSugarsLocal};
use hylic::domain::local as ldom;

fn main() {
    let f = ldom::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let t = ldom::edgy::treeish(|n: &u64| match *n {
        0 => vec![1, 2],
        _ => vec![],
    });

    let p: TreeishPipeline<Local, u64, u64, u64> =
        TreeishPipeline::<Local, _, _, _>::new_local(t, f);

    let r: u64 = p
        .wrap_init(|n: &u64, orig: &dyn Fn(&u64) -> u64| orig(n) + 1)
        .run_from_node(&ldom::FUSED, &0u64);

    // 1, 2, 3 → sum is 1 + 2 + 3 = 6 (wrap_init +1 on each).
    // 0+1, 1+1, 2+1 = 1, 2, 3; root accum = 1 + 2 + 3 = 6.
    assert_eq!(r, 6);
    println!("(local prelude) r = {r}");
    println!("Local prelude minimal check OK.");
}
