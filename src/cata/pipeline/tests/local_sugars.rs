//! Local-domain Stage-2 sugars: wrap_init_local, zipmap_local, etc.
//! Proves the per-D extension impl on LiftedPipeline gives Local
//! users sugar parity with Shared.

use std::cell::RefCell;
use std::rc::Rc;

use crate::cata::pipeline::{PipelineExec, TreeishPipeline, LiftedSugarsLocal};
use crate::domain::{local, Local};

#[test]
fn wrap_init_local_sugar_composes() {
    let init_log: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
    let init_log_for_wrap = init_log.clone();

    let treeish = local::edgy::treeish(|n: &u64| {
        if *n == 0 { vec![1u64, 2] } else if *n == 1 { vec![3u64] } else { vec![] }
    });
    let fold = local::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let pipe = TreeishPipeline::<Local, u64, u64, u64>::new_local(treeish, fold);

    let r = pipe
        .wrap_init(move |n: &u64, orig: &dyn Fn(&u64) -> u64| {
            init_log_for_wrap.borrow_mut().push(*n);
            orig(n) + 100
        })
        .run_from_node(&local::FUSED, &0u64);

    // 0→100, 1→101, 2→102, 3→103. Sum = 406.
    assert_eq!(r, 406);
    let mut log = init_log.borrow().clone();
    log.sort_unstable();
    assert_eq!(log, vec![0, 1, 2, 3]);
}

#[test]
fn zipmap_local_sugar_pairs_result() {
    let treeish = local::edgy::treeish(|n: &u64| if *n == 0 { vec![1u64] } else { vec![] });
    let fold = local::fold(|n: &u64| *n, |h: &mut u64, c: &u64| *h += c, |h: &u64| *h);
    let pipe = TreeishPipeline::<Local, u64, u64, u64>::new_local(treeish, fold);

    let r: (u64, bool) = pipe
        .zipmap(|r: &u64| *r > 0)
        .run_from_node(&local::FUSED, &0u64);
    // 0 + 1 = 1; (1, true).
    assert_eq!(r, (1, true));
}
