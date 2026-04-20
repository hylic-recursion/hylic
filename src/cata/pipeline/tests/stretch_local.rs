//! Stretch-tests for Local: non-Send state captured in wrappers,
//! non-Clone N wrapped in Rc, complex heap types. These scenarios
//! can't run under Funnel because Rc / RefCell aren't Send+Sync.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::cata::pipeline::{PipelineExec, TreeishPipeline, LiftedSugarsLocal};
use crate::domain::{local, Local};

// Non-Clone N, wrapped in Rc.
struct InnerBlob { id: u64, payload: String }

#[test]
fn local_rc_non_clone_payload_with_non_send_fold_state() {
    // Accumulate ids + total payload length into shared mutable
    // state via Rc<RefCell<_>>. Fold sees Rc<InnerBlob> as N.

    let state: Rc<RefCell<(u64, usize)>> = Rc::new(RefCell::new((0, 0)));
    let state_for_init = state.clone();

    let tree: Rc<InnerBlob> = Rc::new(InnerBlob {
        id: 1, payload: "root-payload".into(),
    });
    let kids: HashMap<u64, Vec<Rc<InnerBlob>>> = {
        let mut m = HashMap::new();
        m.insert(1, vec![
            Rc::new(InnerBlob { id: 10, payload: "child-a".into() }),
            Rc::new(InnerBlob { id: 20, payload: "child-b-longer".into() }),
        ]);
        m
    };
    let kids_rc = Rc::new(kids);
    let kids_for_graph = kids_rc.clone();
    let graph = local::edgy::treeish(move |n: &Rc<InnerBlob>| {
        kids_for_graph.get(&n.id).cloned().unwrap_or_default()
    });

    let f: local::Fold<Rc<InnerBlob>, u64, u64> = local::fold(
        move |n: &Rc<InnerBlob>| {
            let mut s = state_for_init.borrow_mut();
            s.0 += n.id;
            s.1 += n.payload.len();
            n.id
        },
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let r = TreeishPipeline::<Local, Rc<InnerBlob>, u64, u64>::new_local(graph, f)
        .run_from_node(&local::FUSED, &tree);
    assert_eq!(r, 31);  // 1 + 10 + 20

    let s = state.borrow();
    assert_eq!(s.0, 31);                           // same id sum
    assert_eq!(s.1, "root-payload".len()
                  + "child-a".len()
                  + "child-b-longer".len());
}

// Local shape-lift with non-Send closure capture.
#[test]
fn local_wrap_accumulate_captures_non_send_state() {
    let counter: Rc<RefCell<Vec<(u64, u64)>>> = Rc::new(RefCell::new(Vec::new()));
    let counter_for_wrap = counter.clone();

    let graph = local::edgy::treeish(|n: &u64| {
        if *n == 0 { vec![1u64, 2] } else if *n == 1 { vec![3u64] } else { vec![] }
    });
    let f = local::fold(
        |n: &u64| *n,
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let r = TreeishPipeline::<Local, u64, u64, u64>::new_local(graph, f)
        .lift()
        .wrap_accumulate(move |h: &mut u64, r: &u64, orig: &dyn Fn(&mut u64, &u64)| {
            counter_for_wrap.borrow_mut().push((*h, *r));
            orig(h, r);
        })
        .run_from_node(&local::FUSED, &0u64);

    assert_eq!(r, 6);
    let log = counter.borrow();
    // wrap_accumulate fires at each child-result accumulation. Tree
    // 0→{1,2}; 1→{3}. 1 accumulates 3; 0 accumulates 1's result and 2's.
    assert!(!log.is_empty());
    assert!(log.len() >= 3);  // at least 3 accumulate calls total
}

// Rc<dyn Trait> as N — non-Send trait objects
use std::fmt::Debug;

trait Summary: Debug {
    fn weight(&self) -> u64;
}

#[derive(Debug)]
struct ConstSummary(u64);
impl Summary for ConstSummary { fn weight(&self) -> u64 { self.0 } }

#[derive(Debug)]
struct SumSummary(Vec<u64>);
impl Summary for SumSummary { fn weight(&self) -> u64 { self.0.iter().sum() } }

#[test]
fn local_rc_trait_object_as_n() {
    let leaves: Rc<Vec<Rc<dyn Summary>>> = Rc::new(vec![
        Rc::new(ConstSummary(5)),
        Rc::new(SumSummary(vec![1, 2, 3])),
    ]);
    let leaves_for_graph = leaves.clone();

    let root: Rc<dyn Summary> = Rc::new(SumSummary(vec![100, 200]));
    let graph = local::edgy::treeish(move |n: &Rc<dyn Summary>| {
        if n.weight() == 300 { leaves_for_graph.iter().cloned().collect() } else { vec![] }
    });
    let f: local::Fold<Rc<dyn Summary>, u64, u64> = local::fold(
        |n: &Rc<dyn Summary>| n.weight(),
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );
    let r = TreeishPipeline::<Local, Rc<dyn Summary>, u64, u64>::new_local(graph, f)
        .run_from_node(&local::FUSED, &root);
    // root: 300; leaves: 5 + 6 = 11. Total: 300 + 11 = 311.
    assert_eq!(r, 311);
}
