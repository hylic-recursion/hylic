//! Phase 5/8: Local shape-lift coverage.
//!
//! Local pipelines don't have the library sugars yet (transforms.rs
//! is Shared-only), so composition uses `then_lift` with a
//! `Shared`-→-`Local`-mirrored constructor directly. The polymorphic
//! `Lift<D, …>` impl on ShapeLift is the same; only the constructor
//! differs.
//!
//! Proves: `Local::wrap_init_lift(w)` produces a ShapeLift<Local,
//! …> that composes into a Local pipeline and runs under Fused.
//! The wrapper `w` captures non-Send state (an `Rc<RefCell<_>>`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::cata::pipeline::{PipelineExec, TreeishPipeline};
use crate::domain::{local, Local};

#[test]
fn local_pipeline_with_wrap_init_lift_non_send_capture() {
    // Non-Send capture inside the wrap_init wrapper: an Rc<RefCell>.
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

    let wrap_lift = Local::wrap_init_lift::<u64, u64, u64, _>(
        move |n: &u64, orig: &dyn Fn(&u64) -> u64| {
            init_log_for_wrap.borrow_mut().push(*n);
            orig(n) + 100
        },
    );

    let r = pipe
        .lift()
        .then_lift(wrap_lift)
        .run_from_node(&local::FUSED, &0u64);

    // init at each node = n + 100. Sum tree rooted at 0:
    // 0: 100, 1: 101, 2: 102, 3: 103. Total = 100+101+102+103 = 406.
    assert_eq!(r, 406);

    // Wrapper fired at each node:
    let mut log = init_log.borrow().clone();
    log.sort_unstable();
    assert_eq!(log, vec![0, 1, 2, 3]);
}
