use super::*;

#[test]
fn pipeline_from_slice() {
    let pipeline = make_pipeline();
    let result = pipeline.run_from_slice(&dom::FUSED, &[0], 0u64);
    let expected = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);
    assert_eq!(result, expected);
}

#[test]
fn pipeline_from_slice_multiple_seeds() {
    let pipeline = make_pipeline();
    let result = pipeline.run_from_slice(&dom::FUSED, &[1, 2], 0u64);
    assert_eq!(result, 6);
}

#[test]
fn pipeline_custom_entry() {
    struct MyTop { roots: Vec<usize> }

    let pipeline = make_pipeline();
    let my_top = MyTop { roots: vec![1, 2] };
    let entry_seeds = graph::edgy_visit({
        let roots = my_top.roots.clone();
        move |_: &(), cb: &mut dyn FnMut(&usize)| {
            for r in &roots { cb(r); }
        }
    });

    let result = pipeline.run(&dom::FUSED, entry_seeds, 0u64);
    assert_eq!(result, 6);
}

#[test]
fn with_lifted_multiple_runs() {
    let pipeline = make_pipeline();
    let entry = graph::edgy_visit(|_: &(), cb: &mut dyn FnMut(&S)| { cb(&0); });

    pipeline.with_lifted(entry, || 0u64, |fold, treeish| {
        let r1 = dom::FUSED.run(fold, treeish, &LiftedNode::Entry);
        let r2 = dom::FUSED.run(fold, treeish, &LiftedNode::Entry);
        assert_eq!(r1, r2);
        assert_eq!(r1, 6);
    });
}

#[test]
fn with_lifted_node_entry() {
    let pipeline = make_pipeline();
    let entry = graph::edgy_visit(|_: &(), _: &mut dyn FnMut(&S)| {});

    pipeline.with_lifted(entry, || 0u64, |fold, treeish| {
        let r = dom::FUSED.run(fold, treeish, &LiftedNode::Node(0));
        assert_eq!(r, 6);
    });
}
